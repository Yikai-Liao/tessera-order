// SPDX-License-Identifier: BSD-2-Clause

import { parseCriterionId, revisionSampling } from "../catalog.ts";
import { summarizePaired, summarizeRatios } from "../statistics.ts";
import type { BuildProvenance } from "../workspace.ts";

export interface LocalBlock {
  baseline: Record<string, number>;
  control: Record<string, number>;
  current: Record<string, number>;
}

/** One deterministic partition of a complete timed-ID listing. */
export interface Shard {
  index: number;
  count: number;
  allCases: string[];
}

/** Balance complete cases across runners without changing their sampling protocol. */
export function shardCases(cases: string[], index: number, count: number) {
  if (
    !Number.isInteger(count) ||
    count < 1 ||
    count > cases.length ||
    !Number.isInteger(index) ||
    index < 1 ||
    index > count ||
    new Set(cases).size !== cases.length
  )
    throw new Error("Invalid benchmark shard");
  const lanes: string[][] = Array.from({ length: count }, () => []);
  const weights = lanes.map(() => 0);
  const weight = (id: string) => {
    const settings = revisionSampling(parseCriterionId(id).len);
    return settings.warm_ms + settings.window_ms;
  };
  for (const id of cases.toSorted((a, b) => weight(b) - weight(a) || a.localeCompare(b))) {
    const lane = weights.indexOf(Math.min(...weights));
    lanes[lane].push(id);
    weights[lane] += weight(id);
  }
  return lanes[index - 1].sort();
}

export interface LocalRun {
  schema: 1;
  protocol?: "paired-samples-v1" | "paired-samples-v2";
  samples?: Record<string, { baseline: number; current: number }[]>;
  baseline: string;
  head: string;
  dirty: boolean;
  source: { baseline: string; current: string; harness: string; catalog: string };
  binaries: { baseline: string; current: string };
  builds?: { baseline: BuildProvenance; current: BuildProvenance };
  rustc: string;
  flags: string;
  configuration: string;
  host: {
    platform: string;
    release: string;
    arch: string;
    cpu: string;
    affinity: string;
    aslr?: "disabled" | "uncontrolled";
  };
  cases: string[];
  shard?: Shard;
  blocks: LocalBlock[];
}

export interface LocalSummary {
  id: string;
  ratio: number;
  low: number;
  high: number;
  aaLow?: number;
  aaHigh?: number;
  verdict: "faster" | "slower" | "within noise";
}

/** Summarize retained sample pairs, or historical nine-block A/A evidence. */
export function summarizeLocal(
  run: Pick<LocalRun, "protocol" | "samples" | "cases" | "blocks">,
): LocalSummary[] {
  if (run.protocol === "paired-samples-v1" || run.protocol === "paired-samples-v2") {
    if (
      run.blocks.length ||
      !run.samples ||
      !run.cases.length ||
      new Set(run.cases).size !== run.cases.length ||
      Object.keys(run.samples).sort().join("\n") !== [...run.cases].sort().join("\n")
    )
      throw new Error("Incomplete paired samples");
    return run.cases.map((id) => {
      const samples = run.samples![id];
      const len = parseCriterionId(id).len;
      const count = revisionSampling(len).samples;
      if (
        samples.length !== count ||
        samples.some(
          (s) => ![s.baseline, s.current].every((value) => Number.isFinite(value) && value > 0),
        )
      )
        throw new Error("Invalid paired sample budget or observation");
      return { id, ...summarizeRatios(samples.map((s) => s.current / s.baseline)) };
    });
  }
  if (run.protocol !== undefined || run.samples !== undefined)
    throw new Error("Unsupported comparison protocol");
  if (run.blocks.length !== 9 || !run.cases.length || new Set(run.cases).size !== run.cases.length)
    throw new Error("Expected nine complete blocks");
  for (const block of run.blocks) {
    for (const values of [block.baseline, block.control, block.current]) {
      if (
        Object.keys(values).sort().join("\n") !== [...run.cases].sort().join("\n") ||
        Object.values(values).some((v) => !Number.isFinite(v) || v <= 0)
      ) {
        throw new Error("Block results do not match the selected cases");
      }
    }
  }
  return run.cases.map((id) => ({
    id,
    ...summarizePaired(
      run.blocks.map((block) => ({
        first: block.baseline[id],
        second: block.control[id],
        value: block.current[id],
      })),
    ),
  }));
}
