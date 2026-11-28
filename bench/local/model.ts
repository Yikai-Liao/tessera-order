// SPDX-License-Identifier: BSD-2-Clause

import { readFile } from "node:fs/promises";
import { join } from "node:path";
import {
  executionThreads,
  parseCriterionId,
  referenceColumn,
  revisionSampling,
} from "../catalog.ts";
import { type Plan, parsePlan, planExecutions, planOperations } from "../plan.ts";
import { summarizeRatios } from "../statistics.ts";
import type { GoBuild } from "../tracking/result.ts";
import type { BuildProvenance } from "../workspace.ts";
import type { GoRequests } from "./go.ts";

/** Revision builds replace only the Tessera library, so a revision comparison measures only
 * Tessera, at the plan's executions; any other implementation would be identical code in both
 * builds. Parallel executions require an operation with a parallel entry point. Returns the CPUs
 * the scheduler must reserve: the largest planned worker count, or one for serial-only plans. */
export function revisionThreads(plan: Plan, language: "rust" | "go" = "rust"): number {
  const implementation = language === "go" ? "go-tessera" : "tessera";
  if (plan.implementations.join() !== implementation)
    throw new Error(
      `Revision comparisons measure only ${implementation}; select executions instead`,
    );
  if (
    plan.include.some(
      (rule) =>
        rule.operation &&
        !referenceColumn(rule.operation, "t1") &&
        rule.executions?.some((execution) => execution !== "serial"),
    )
  )
    throw new Error(
      "Revision comparisons of a serial-only operation measure only serial executions",
    );
  return Math.max(1, ...planExecutions(plan).map((execution) => executionThreads(execution) ?? 1));
}

/** Whether a plan measures any operation other than full sorting, which needs the harness's
 * `operations` feature in both builds; baselines before 0.2 do not compile with it. */
export function isOperation(plan: Plan) {
  return planOperations(plan).some((operation) => operation.kind !== "sort");
}

/** Whether a plan measures any parallel entry point, which needs Rayon in both builds. */
export function isParallel(plan: Plan) {
  return planExecutions(plan).some((execution) => execution !== "serial");
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

/** Immutable source and executable identity, shared by preparation and measurement. */
export interface RevisionPair {
  schema: 1;
  protocol: "paired-samples-v2";
  baseline: string;
  head: string;
  dirty: boolean;
  source: { baseline: string; current: string; harness: string; catalog: string };
  binaries: { baseline: string; current: string };
  /** Executed files relative to the artifact directory. */
  binaryPaths: { baseline: string; current: string };
  builds?: { baseline: BuildProvenance; current: BuildProvenance };
  go?: { builds: { baseline: GoBuild; current: GoBuild }; requests: GoRequests };
  rustc?: string;
  flags?: string;
  configuration: string;
  cases: string[];
}

/** Complete build inventory and plan; no measurement host, samples or shard. */
export interface PreparedPair extends RevisionPair {
  kind: "prepared-revisions";
  plan: Plan;
}

/** Measurement state; empty samples are incomplete, never a prepared build. */
export interface LocalRun extends RevisionPair {
  samples: Record<string, { baseline: number; current: number }[]>;
  sampleCounts: Record<string, number>;
  host: {
    platform: string;
    release: string;
    arch: string;
    cpu: string;
    affinity: string;
    aslr?: "disabled" | "uncontrolled";
    /** CPUs of concurrent serial lanes, one per physical core; absent for one lane. */
    serialLanes?: string[];
  };
  shard?: Shard;
}

export function assertGoBuilds(builds: NonNullable<RevisionPair["go"]>["builds"]) {
  if (JSON.stringify(builds.baseline) !== JSON.stringify(builds.current))
    throw new Error("Go revision build settings differ");
  if (
    !builds.current?.GoVersion ||
    !builds.current.Settings?.some(({ Key, Value }) => Key === "GOEXPERIMENT" && Value === "simd")
  )
    throw new Error("Expected a Go SIMD benchmark build");
}

/** Read frozen preparation metadata. Execution must separately admit its plan. */
export async function readPrepared(directory: string): Promise<PreparedPair> {
  const pair = JSON.parse(await readFile(join(directory, "prepared.json"), "utf8"));
  if (
    pair.kind !== "prepared-revisions" ||
    pair.schema !== 1 ||
    pair.protocol !== "paired-samples-v2" ||
    ["host", "shard", "samples", "sampleCounts"].some((field) => field in pair) ||
    !Array.isArray(pair.cases) ||
    !pair.cases.length ||
    new Set(pair.cases).size !== pair.cases.length ||
    (!pair.go && (!pair.builds?.baseline || !pair.builds?.current)) ||
    !pair.binaryPaths?.baseline ||
    !pair.binaryPaths?.current
  )
    throw new Error("Expected an unmeasured prepared revision pair");
  if (pair.go) assertGoBuilds(pair.go.builds);
  pair.plan = parsePlan(pair.plan);
  revisionThreads(pair.plan, pair.go ? "go" : "rust");
  return pair;
}

export interface LocalSummary {
  id: string;
  ratio: number;
  low: number;
  high: number;
  verdict: "faster" | "slower" | "within noise";
}

/** Summarize the current protocol using its recorded sample budgets. */
export function summarizeLocal(
  run: Pick<LocalRun, "protocol" | "samples" | "sampleCounts" | "cases">,
): LocalSummary[] {
  if (run.protocol !== "paired-samples-v2") throw new Error("Unsupported comparison protocol");
  const { samples, sampleCounts, cases } = run;
  if (
    !samples ||
    !sampleCounts ||
    !cases.length ||
    new Set(cases).size !== cases.length ||
    Object.keys(samples).sort().join("\n") !== [...cases].sort().join("\n") ||
    Object.keys(sampleCounts).sort().join("\n") !== [...cases].sort().join("\n")
  )
    throw new Error("Incomplete paired samples or recorded sample budgets");
  return cases.map((id) => {
    const count = sampleCounts[id];
    const observations = samples[id];
    if (
      !Number.isSafeInteger(count) ||
      count < 10 ||
      !Array.isArray(observations) ||
      observations.length !== count ||
      observations.some((s) => ![s.baseline, s.current].every((v) => Number.isFinite(v) && v > 0))
    )
      throw new Error("Invalid paired sample budget or observation");
    return { id, ...summarizeRatios(observations.map((s) => s.current / s.baseline)) };
  });
}
