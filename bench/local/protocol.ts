// SPDX-License-Identifier: BSD-2-Clause

import { median } from "../statistics.ts";
import type { BuildProvenance } from "../workspace.ts";
import { parseLayoutSeeds } from "./layout.ts";
import { type LocalBlock, type LocalRun, validateBlock } from "./model.ts";

export const PROTOCOL_TRIALS = 9;
export const PROTOCOL_LAYOUTS = 3;
export const ALERT_FRACTION = 0.05;
export type ExpectedDirection = "same" | "faster" | "slower";

export interface ProtocolPair {
  baselineSeed: number;
  currentSeed: number;
  binaries: { baseline: string; current: string };
  builds: { baseline: BuildProvenance; current: BuildProvenance };
}

export interface ProtocolTrial {
  pairs: ProtocolPair[];
  singlePair: number;
  single?: LocalBlock;
  multiple: LocalBlock[];
  wallMs: { single?: number; multiple?: number };
}

export interface ProtocolRun {
  schema: 3;
  protocol: "candidate-only-vs-paired-median3-v1";
  expected: ExpectedDirection;
  controls: string[];
  timings?: { single: Record<string, StudyTiming>; multiple: Record<string, StudyTiming> };
  provenance: Omit<LocalRun, "schema" | "blocks" | "binaries" | "builds">;
  linker: { driver: string; version: string };
  trials: ProtocolTrial[];
}

export interface StudyTiming {
  warm_up_ms: number;
  measurement_ms: number;
  samples: number;
}

/** Reserve six distinct layouts per trial, independently of the previous calibration. */
export function protocolSeeds(firstSeed: number) {
  const seeds = Array.from(
    { length: PROTOCOL_TRIALS * PROTOCOL_LAYOUTS * 2 },
    (_, index) => firstSeed + index,
  );
  parseLayoutSeeds(seeds.join(","));
  return seeds;
}

/** Check source expectations and all prepared layouts before starting measurements. */
export function validateProtocolBuilds(run: ProtocolRun) {
  if (
    run.schema !== 3 ||
    run.protocol !== "candidate-only-vs-paired-median3-v1" ||
    run.trials.length !== PROTOCOL_TRIALS
  )
    throw new Error("Expected nine protocol trials");
  if (!["same", "faster", "slower"].includes(run.expected))
    throw new Error("Invalid expected direction");
  const source = run.provenance.source;
  if ((run.expected === "same") !== (source.baseline === source.current))
    throw new Error(
      "Source hashes do not match the declared same-source or changed-source experiment",
    );
  const pairs = run.trials.flatMap((trial) => {
    if (trial.pairs.length !== PROTOCOL_LAYOUTS)
      throw new Error("Expected three revision pairs per trial");
    if (
      !Number.isInteger(trial.singlePair) ||
      trial.singlePair < 0 ||
      trial.singlePair >= PROTOCOL_LAYOUTS
    )
      throw new Error("Invalid single-layout pair selection");
    return trial.pairs;
  });
  parseLayoutSeeds(pairs.flatMap((pair) => [pair.baselineSeed, pair.currentSeed]).join(","));
  const hashes = pairs.flatMap((pair) => [pair.binaries.baseline, pair.binaries.current]);
  if (new Set(hashes).size !== hashes.length)
    throw new Error("Protocol layouts must have distinct executables");
  for (const pair of pairs) {
    for (const build of [pair.builds.baseline, pair.builds.current]) {
      if (!build.features.includes("CARGO_FEATURE_LAYOUT_STUDY"))
        throw new Error("Protocol builds require the layout-study feature");
      if (
        build.rustc !== run.provenance.rustc ||
        build.cargo_encoded_rustflags !== run.provenance.flags
      )
        throw new Error("Protocol build settings differ");
    }
  }
}

/** Classify a trial using the existing fixed ±5% latency limits. */
function direction(ratio: number) {
  return ratio < 1 - ALERT_FRACTION
    ? "faster"
    : ratio > 1 + ALERT_FRACTION
      ? "slower"
      : "within limits";
}

/** Validate effective windows and case selection before spending measurement time. */
export function validateProtocolSettings(run: ProtocolRun) {
  const cases = run.provenance.cases;
  if (!cases.length) throw new Error("Expected selected protocol cases");
  if (run.controls.some((id) => !cases.includes(id)))
    throw new Error("Unknown protocol control case");
  if (
    !run.timings ||
    [run.timings.single, run.timings.multiple].some(
      (settings) => Object.keys(settings).sort().join("\n") !== [...cases].sort().join("\n"),
    )
  )
    throw new Error("Missing effective timing settings");
  for (const id of cases) {
    const single = run.timings.single[id],
      multiple = run.timings.multiple[id];
    if (
      [
        single.warm_up_ms,
        single.measurement_ms,
        single.samples,
        multiple.warm_up_ms,
        multiple.measurement_ms,
        multiple.samples,
      ].some((value) => !Number.isSafeInteger(value) || value <= 0) ||
      single.samples < 10 ||
      multiple.warm_up_ms !== Math.ceil(single.warm_up_ms / 6) ||
      multiple.measurement_ms !== Math.ceil(single.measurement_ms / 6) ||
      multiple.samples !== Math.max(10, Math.ceil(single.samples / 6))
    )
      throw new Error("Invalid full or divided timing settings");
  }
}

/** Compare independent-layout alerts and elapsed costs without fitting a noise threshold. */
export function summarizeProtocols(run: ProtocolRun) {
  validateProtocolBuilds(run);
  validateProtocolSettings(run);
  const cases = run.provenance.cases;
  for (const trial of run.trials) {
    if (
      !trial.single ||
      trial.multiple.length !== PROTOCOL_LAYOUTS ||
      trial.multiple.some((block) => !block)
    )
      throw new Error("Protocol reports require nine complete trials");
    for (const block of [trial.single, ...trial.multiple]) {
      validateBlock(cases, block);
      if (
        !block.elapsedMs ||
        ["baseline", "control", "current"].some((role) => {
          const value = block.elapsedMs![role as keyof NonNullable<LocalBlock["elapsedMs"]>];
          return !Number.isFinite(value) || value <= 0;
        })
      )
        throw new Error("Protocol blocks require positive elapsed times for every process");
    }
  }
  const processTotal = (blocks: LocalBlock[]) =>
    blocks.reduce((total, block) => {
      const elapsed = block.elapsedMs!;
      return total + elapsed.baseline + elapsed.control + elapsed.current;
    }, 0);
  const wallTimes = run.trials.map((trial) => {
    const { single: singleMs, multiple: multipleMs } = trial.wallMs ?? {};
    if (
      typeof singleMs !== "number" ||
      typeof multipleMs !== "number" ||
      !Number.isFinite(singleMs) ||
      !Number.isFinite(multipleMs) ||
      singleMs <= 0 ||
      multipleMs <= 0
    )
      throw new Error("Protocol trials require complete strategy wall times");
    if (singleMs < processTotal([trial.single!]) || multipleMs < processTotal(trial.multiple))
      throw new Error("Strategy wall time cannot be smaller than its recorded processes");
    return { singleMs, multipleMs };
  });
  const wallRatios = wallTimes.map((trial) => trial.multipleMs / trial.singleMs);
  const jobCosts = run.trials.map((trial) => ({
    singleMs: trial.single!.elapsedMs!.current,
    multipleMs: trial.multiple.reduce(
      (total, block) => total + block.elapsedMs!.baseline + block.elapsedMs!.current,
      0,
    ),
  }));
  const costRatios = jobCosts.map((trial) => trial.multipleMs / trial.singleMs);
  return {
    expected: run.expected,
    trials: PROTOCOL_TRIALS,
    alertFraction: ALERT_FRACTION,
    wallTime: {
      ratio: median(wallRatios),
      low: Math.min(...wallRatios),
      high: Math.max(...wallRatios),
      trials: wallTimes,
    },
    jobCost: {
      ratio: median(costRatios),
      low: Math.min(...costRatios),
      high: Math.max(...costRatios),
      fitsObservedBudget: median(costRatios) <= 1,
      trials: jobCosts,
    },
    cases: cases.map((id) => {
      const expected = run.controls.includes(id) ? "same" : run.expected;
      const summarize = (select: (trial: ProtocolTrial) => LocalBlock[]) => {
        const observations = run.trials.map((trial) => {
          const blocks = select(trial);
          const anchors = blocks.map((block) => (block.baseline[id] + block.control[id]) / 2);
          const ratio =
            median(blocks.map((block) => block.current[id])) /
            median(blocks.map((block) => block.baseline[id]));
          return {
            ratio,
            direction: direction(ratio),
            aa: blocks.flatMap((block, index) => [
              block.baseline[id] / anchors[index],
              block.control[id] / anchors[index],
            ]),
          };
        });
        const faster = observations.filter((value) => value.direction === "faster").length;
        const slower = observations.filter((value) => value.direction === "slower").length;
        return {
          ratio: median(observations.map((value) => value.ratio)),
          faster,
          slower,
          withinLimits: PROTOCOL_TRIALS - faster - slower,
          matchesExpectation:
            expected === "same"
              ? PROTOCOL_TRIALS - faster - slower
              : expected === "faster"
                ? faster
                : slower,
          aaLow: Math.min(...observations.flatMap((value) => value.aa)),
          aaHigh: Math.max(...observations.flatMap((value) => value.aa)),
          observations: observations.map(({ ratio, direction }) => ({ ratio, direction })),
        };
      };
      return {
        id,
        expected,
        single: summarize((trial) => [trial.single!]),
        multiple: summarize((trial) => trial.multiple),
      };
    }),
  };
}
