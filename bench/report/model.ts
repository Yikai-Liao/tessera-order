// SPDX-License-Identifier: BSD-2-Clause

import {
  type CaseIdentity,
  caseId,
  catalog,
  comparisonControl,
  type Execution,
  isCollection,
  operationComparison,
  parseCaseId,
} from "../catalog.ts";
import type { Plan } from "../plan.ts";

export { caseId } from "../catalog.ts";

import { median, type PairedVerdict, summarizePaired } from "../statistics.ts";

export const { summaryDistributions: distributions, types } = operationComparison("sort")!;
export const { lengths } = catalog.inputs;
/** The serial Tessera control of every implementation comparison. */
export const TESSERA = "tessera/serial";
/** The repeated serial Tessera control, measured in the same blocks. */
export const TESSERA_AA = "tessera-aa";
export function pairedColumns(operation: CaseIdentity["operation"]) {
  return {
    control: comparisonControl(operation) + "/serial",
    repeat: isCollection(operation) ? "reference-aa" : TESSERA_AA,
  };
}
export interface Case extends CaseIdentity {
  /** Measured column IDs: planned columns whose adapters support this case. */
  columns: string[];
  work_items?: number;
  input_entries?: number;
  cold_queries?: number;
  churn_pairs?: number;
  /** Planned columns whose adapters do not support this case. */
  omitted: { column: string; reason: string }[];
  /** Each measured column's route: the exact call it makes. */
  routes: Record<string, string>;
}
export interface ManifestColumn {
  id: string;
  implementation: string;
  execution: Execution;
  /** Stability of this column's route for the session's operation. */
  stable: boolean;
  adapter: string;
}
export interface Manifest {
  schema: number;
  plan_name: string;
  plan: Plan;
  index_bytes?: number;
  build: string;
  blocks: number;
  cases: Case[];
  columns: ManifestColumn[];
  commit: string;
  dirty: boolean;
  rustc: string;
  source_sha256: string;
  binary_sha256: string;
  cargo_lock: string;
  features: string[];
  target: string;
  profile: string;
  opt_level: string;
  debug: string;
  cc?: string;
  cxx?: string;
  dispatch?: Record<string, string>;
  [measurementId: string]: unknown;
}
export interface Sample {
  kind: "sample";
  case_id: string;
  /** Column ID, or `tessera-aa`. */
  column: string;
  build: string;
  group: string;
  block: number;
  slot: number;
  batch: number;
  elapsed_ns: number;
  verified: boolean;
}
export interface Observation {
  kind: "observation";
  group: string;
  build: string;
  phase: "start" | "end";
  unix_seconds: number;
  load: string | null;
  frequency_khz: string | null;
  temperatures_millidegrees: number[];
  memory_pressure: string | null;
}
export interface Memory {
  case_id: string;
  column: string;
  build: string;
  peak_extra_bytes: number;
  retained_bytes?: number;
  live_entries?: number;
  /** Mandatory output allocation included in the peak; zero for in-place operations. */
  output_bytes?: number;
  verified: boolean;
}
export interface Dataset {
  host: Record<string, unknown>;
  manifest: Manifest;
  samples: Sample[];
  observations: Observation[];
  memory: Memory[];
  partial: boolean;
}
export function measurementId(build: string, id: string, column: string) {
  return `${build}:${id}:${column}`;
}
export interface Summary {
  caseId: string;
  column: string;
  build: string;
  stable: boolean;
  ratios: number[];
  ratio: number;
  low: number;
  high: number;
  aaLow: number;
  aaHigh: number;
  ns: number;
  nsPerEntry?: number;
  verdict: PairedVerdict;
}
export function summarize(dataset: Dataset): Map<string, Summary> {
  const groups = Map.groupBy(dataset.samples, (s) => measurementId(s.build, s.case_id, s.column));
  const result = new Map<string, Summary>();
  const { control, repeat } = pairedColumns(dataset.manifest.cases[0].operation);
  const cases = new Map(dataset.manifest.cases.map((c) => [caseId(c), c]));
  for (const unsorted of groups.values()) {
    const samples = [...unsorted].sort((a, b) => a.block - b.block);
    const { build, case_id: id, column } = samples[0];
    const len = cases.get(id)?.work_items ?? parseCaseId(id).len;
    const a = groups.get(measurementId(build, id, control))!;
    const b = groups.get(measurementId(build, id, repeat))!;
    const time = (s: Sample) => s.elapsed_ns / s.batch;
    const times = (values: Sample[]) => new Map(values.map((s) => [s.block, time(s)]));
    const first = times(a),
      second = times(b);
    const paired = summarizePaired(
      samples.map((sample) => ({
        first: first.get(sample.block)!,
        second: second.get(sample.block)!,
        value: time(sample),
      })),
      dataset.manifest.blocks,
    );
    if (column === repeat) {
      paired.ratios = samples.map((sample) => time(sample) / first.get(sample.block)!);
      paired.ratio = median(paired.ratios);
      paired.low = Math.min(...paired.ratios);
      paired.high = Math.max(...paired.ratios);
    }
    if (column === control || column === repeat) paired.verdict = "within noise";
    const measuredColumn = column === repeat ? control : column;
    const ns = median(samples.map((s) => time(s) / len));
    const input = cases.get(id);
    result.set(measurementId(build, id, column), {
      caseId: id,
      build,
      column,
      stable: dataset.manifest.columns.find((c) => c.id === measuredColumn)!.stable,
      ...paired,
      ns,
      ...(input?.operation.kind === "map-iterate" ? { nsPerEntry: ns / input.len } : {}),
    });
  }
  return result;
}
