// SPDX-License-Identifier: BSD-2-Clause

import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";
import {
  caseLabel,
  caseParts,
  parseCriterionId,
  revisionSampling,
  type Variant,
} from "../catalog.ts";
import { type LocalRun, shardCases, summarizeLocal } from "../local/model.ts";
import { selectedCases } from "../local/runner.ts";
import { loadPlan, parsePlan } from "../plan.ts";
import { buildBenchmark, withWorkspace } from "../workspace.ts";
import { type BencherReport, type Row, summarizeRows } from "./report.ts";

/** Hosted matrix width; workflows obtain their matrix from this owner. */
export const CONTINUOUS_SHARDS = 6;

/** Exact source identity resolved independently by the trusted publisher. */
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

/** Resolve the trusted selection with the existing Rust plan/threshold owner.
 *
 * Publication calls this only from its trusted checkout, before reading PR
 * artifacts. Listing generates no inputs and performs no timed measurement.
 */
export async function continuousInventory(root: string) {
  return withWorkspace(root, async ({ tree, target }) => {
    const built = await buildBenchmark(tree, target, "smoke", ["rayon"]);
    return selectedCases(
      { baseline: built.executable, current: built.executable },
      join(tree, "bench/plans/continuous.toml"),
      undefined,
      tree,
      true,
    );
  });
}

/** Validate complete, disjoint shards before any upload or passing check.
 *
 * Results are untrusted JSON, never executable inputs. All shards must use one
 * harness, catalog, source pair and toolchain; runner identities may differ.
 */
export async function readPaired(
  directory: string,
  variant: Variant,
  source: PairedSource,
  expectedCases: string[],
) {
  const runs: LocalRun[] = [];
  const expectedPlan = JSON.stringify(await loadPlan("continuous"));
  for (const name of await readdir(directory)) {
    if (!name.startsWith(`paired-${variant.id}-`)) continue;
    const plan = parsePlan(JSON.parse(await readFile(join(directory, name, "plan.json"), "utf8")));
    if (JSON.stringify(plan) !== expectedPlan) throw new Error("Unexpected continuous plan");
    const run = JSON.parse(await readFile(join(directory, name, "local.json"), "utf8")) as LocalRun;
    runs.push(run);
  }
  if (!runs.length) throw new Error("No completed measurement shards");
  const reference = runs[0];
  const count = reference.shard?.count;
  if (count !== CONTINUOUS_SHARDS || count !== runs.length)
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
      run.configuration !== variant.id ||
      !shard ||
      shard.count !== count ||
      indexes.has(shard.index) ||
      JSON.stringify(shard.allCases) !== JSON.stringify(expectedCases) ||
      JSON.stringify(run.source) !== JSON.stringify(reference.source) ||
      JSON.stringify(settings(run)) !== JSON.stringify(settings(reference)) ||
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
        variant.features
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

/** Check that Cloud retained this run’s exact configuration and recomputed ratios. */
export function validateCloud(
  report: BencherReport,
  runs: LocalRun[],
  variant: Variant,
  number?: string,
) {
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
    report.results.length !== 1 ||
    report.branch.head.version?.hash !== runs[0].head ||
    report.testbed.name !== variant.relativeTestbed ||
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
export function pairedSummary(runs: LocalRun[], variant: Variant, cloud?: BencherReport) {
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
    testbed: cloud?.testbed ?? { uuid: "", name: variant.relativeTestbed },
    project: cloud?.project.slug ?? "",
    uuid: cloud?.uuid ?? "",
    relative: true,
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
