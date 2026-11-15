// SPDX-License-Identifier: BSD-2-Clause

import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";
import {
  caseLabel,
  caseParts,
  continuous,
  parseCriterionId,
  revisionSampling,
} from "../catalog.ts";
import {
  type LocalRun,
  pairedCaseBudgetMs,
  readPrepared,
  revisionFeatures,
  shardCases,
  summarizeLocal,
} from "../local/model.ts";
import { parsePlan } from "../plan.ts";
import { type BencherReport, type Row, summarizeRows } from "./report.ts";

/** Hosted capacity ceiling; small selections need fewer runners. */
const MAX_SHARDS = 6;
// Target half the four-minute end-to-end budget for nominal measurement
// per runner; startup, checking and scheduling add to this, so it is not an SLA.
const SHARD_BUDGET_MS = 120_000;

/** Derive the hosted matrix from the frozen inventory without reducing samples. */
export function continuousShards(cases: string[]) {
  if (!cases.length) throw new Error("No continuous cases selected");
  const budget = cases.reduce((sum, id) => sum + pairedCaseBudgetMs(id), 0);
  return Math.min(MAX_SHARDS, cases.length, Math.ceil(budget / SHARD_BUDGET_MS));
}

/** Exact source identity captured by this CI run. */
export interface PairedSource {
  hash: string;
  baseHash: string;
}

function settings(run: LocalRun) {
  const select = (build: NonNullable<LocalRun["builds"]>["current"]) => ({
    rustc: build.rustc,
    target: build.target,
    profile: build.profile,
    flags: build.cargo_encoded_rustflags,
    opt: build.opt_level,
    debug: build.debug,
    features: build.features,
    args: build.rustc_args,
  });
  if (!run.builds) throw new Error("Missing build provenance");
  return { baseline: select(run.builds.baseline), current: select(run.builds.current) };
}

/** Validate complete, disjoint shards before any upload or passing check.
 *
 * This run’s prepared pair owns the plan, full inventory and build identity.
 * Only JSON is read; measurement hosts may differ from the preparation host.
 */
export async function readPaired(directory: string, source: PairedSource, prepared: string) {
  const runs: LocalRun[] = [];
  const reference = await readPrepared(prepared);
  const expectedCases = reference.cases;
  const expectedPlan = JSON.stringify(reference.plan);
  if (reference.head !== source.hash || reference.baseline !== source.baseHash || reference.dirty)
    throw new Error("Expected this run’s unmeasured prepared pair");
  for (const name of await readdir(directory)) {
    if (!name.startsWith("paired-")) continue;
    const plan = parsePlan(JSON.parse(await readFile(join(directory, name, "plan.json"), "utf8")));
    if (JSON.stringify(plan) !== expectedPlan) throw new Error("Unexpected continuous plan");
    const run = JSON.parse(await readFile(join(directory, name, "local.json"), "utf8")) as LocalRun;
    runs.push(run);
  }
  if (!runs.length) throw new Error("No completed measurement shards");
  const count = runs[0].shard?.count;
  if (count !== continuousShards(expectedCases) || count !== runs.length)
    throw new Error("Missing measurement shards");
  const indexes = new Set<number>();
  for (const run of runs) {
    const shard = run.shard;
    if (
      run.schema !== 1 ||
      run.protocol !== "paired-samples-v2" ||
      run.host.aslr !== "disabled" ||
      run.dirty ||
      run.head !== source.hash ||
      run.baseline !== source.baseHash ||
      !shard ||
      shard.count !== count ||
      indexes.has(shard.index) ||
      JSON.stringify(shard.allCases) !== JSON.stringify(expectedCases) ||
      JSON.stringify(run.source) !== JSON.stringify(reference.source) ||
      JSON.stringify(run.binaries) !== JSON.stringify(reference.binaries) ||
      JSON.stringify(run.builds) !== JSON.stringify(reference.builds) ||
      JSON.stringify(run.cases) !== JSON.stringify(shardCases(shard.allCases, shard.index, count))
    )
      throw new Error("Incompatible, duplicate or misattributed measurement shards");
    if (
      !run.builds ||
      ![
        run.source.baseline,
        run.source.current,
        run.source.harness,
        run.source.catalog,
        run.binaries.baseline,
        run.binaries.current,
      ].every((value) => typeof value === "string" && /^[a-f0-9]{64}$/.test(value)) ||
      JSON.stringify(settings(run).baseline) !== JSON.stringify(settings(run).current) ||
      run.builds.current.features.join(",") !==
        revisionFeatures(reference.plan)
          .map((feature) => `CARGO_FEATURE_${feature.toUpperCase().replaceAll("-", "_")}`)
          .toSorted()
          .join(",")
    )
      throw new Error("Missing source or configuration provenance");
    for (const id of run.cases) {
      const { mode, len } = parseCriterionId(id);
      if (mode !== "api" && mode !== "par-4") throw new Error("Unsupported measurement mode");
      if (run.sampleCounts?.[id] !== revisionSampling(len).samples)
        throw new Error("Untrusted measurement budget");
    }
    summarizeLocal(run);
    indexes.add(shard.index);
  }
  return runs.toSorted((a, b) => a.shard!.index - b.shard!.index);
}

/** Check that Cloud retained this run’s exact Testbed, branch and recomputed ratios. */
export function validateCloud(report: BencherReport, runs: LocalRun[], number?: string) {
  const expected = pairedBmf(runs);
  const values = new Map<string, number>();
  for (const result of report.results?.[0] ?? []) {
    if (
      result.measures.length !== 1 ||
      result.measures[0].measure.name !== "paired-time-ratio" ||
      values.has(result.benchmark.name)
    )
      throw new Error("Cloud report has duplicate or unexpected measures");
    values.set(result.benchmark.name, result.measures[0].metric.value);
  }
  if (
    !report.uuid ||
    !report.branch.head.uuid ||
    report.results.length !== 1 ||
    report.branch.head.version?.hash !== runs[0].head ||
    report.testbed.name !== continuous.testbed ||
    report.branch.name !== (number ? `pr-${number}` : "main") ||
    values.size !== Object.keys(expected).length ||
    Object.entries(expected).some(([id, metric]) => {
      const value = values.get(id);
      return (
        value === undefined ||
        !Number.isFinite(value) ||
        Math.abs(value / metric["paired-time-ratio"].value - 1) > 1e-10
      );
    })
  )
    throw new Error("Cloud report does not match the paired measurement");
}

/** Derive ratios and verdicts from raw paired samples, never from PR-authored summaries. */
export function pairedSummary(runs: LocalRun[], cloud?: BencherReport) {
  const results = runs.flatMap(summarizeLocal);
  const rows: Row[] = results.map((result) => ({
    name: result.id,
    parts: caseParts(result.id),
    label: caseLabel(result.id),
    benchmark: result.id,
    measure: "paired-time-ratio",
    value: result.ratio,
    baseline: 1,
    lower: result.low,
    upper: result.high,
    change: result.ratio - 1,
    verdict:
      result.verdict === "slower"
        ? "regression"
        : result.verdict === "faster"
          ? "improvement"
          : "within",
  }));
  return summarizeRows(rows, {
    hash: runs[0].head,
    baseHash: runs[0].baseline,
    history: cloud ? [{ branch: cloud.branch.uuid, head: cloud.branch.head.uuid }] : [],
    testbed: cloud?.testbed ?? { uuid: "", name: continuous.testbed },
    project: cloud?.project.slug ?? "",
    uuid: cloud?.uuid ?? "",
  });
}

/** Retain screen ratios and baseline brackets, never absolute-latency history. */
export function pairedBmf(runs: LocalRun[]) {
  return Object.fromEntries(
    runs.flatMap(summarizeLocal).map((result) => [
      result.id,
      {
        "paired-time-ratio": {
          value: result.ratio,
          lower_value: result.low,
          upper_value: result.high,
        },
      },
    ]),
  );
}
