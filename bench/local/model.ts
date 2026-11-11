// SPDX-License-Identifier: BSD-2-Clause

import { parseCriterionId, revisionSampling } from "../catalog.ts";
import { summarizeRatios } from "../statistics.ts";
import type { BuildProvenance } from "../workspace.ts";

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
  protocol: "paired-samples-v2";
  samples?: Record<string, { baseline: number; current: number }[]>;
  sampleCounts?: Record<string, number>;
  baseline: string;
  head: string;
  dirty: boolean;
  source: { baseline: string; current: string; harness: string; catalog: string };
  binaries: { baseline: string; current: string };
  /** Executed files relative to the result directory; absent from older artifacts. */
  binaryPaths?: { baseline: string; current: string };
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
