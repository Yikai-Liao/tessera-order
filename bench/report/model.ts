// SPDX-License-Identifier: BSD-2-Clause

import { catalog } from "../catalog.ts";
import { median, summarizePaired } from "../statistics.ts";

export const { shapes, types, lengths } = catalog.comparison;
export interface Case {
  ty: string;
  distribution: string;
  len: number;
  variants: string[];
}
export interface Manifest {
  schema: number;
  suite: string;
  suite_version: number;
  build: string;
  blocks: number;
  cases: Case[];
  variants: { name: string; stable: boolean }[];
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
  variant: string;
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
  variant: string;
  build: string;
  peak_extra_bytes: number;
  verified: boolean;
}
export interface Dataset {
  host: Record<string, unknown>;
  manifests: Map<string, Manifest>;
  samples: Sample[];
  observations: Observation[];
  memory: Memory[];
  partial: boolean;
}
export function caseId(c: Pick<Case, "ty" | "distribution" | "len">) {
  return `${c.ty}/${c.distribution}/${c.len}/s1`;
}
export function measurementId(build: string, id: string, variant: string) {
  return `${build}:${id}:${variant}`;
}
export interface Summary {
  caseId: string;
  variant: string;
  build: string;
  stable: boolean;
  ratios: number[];
  ratio: number;
  low: number;
  high: number;
  aaLow: number;
  aaHigh: number;
  ns: number;
  verdict: "faster" | "slower" | "within noise";
}
export function summarize(dataset: Dataset): Map<string, Summary> {
  const groups = Map.groupBy(dataset.samples, (s) => measurementId(s.build, s.case_id, s.variant));
  const result = new Map<string, Summary>();
  for (const unsorted of groups.values()) {
    const samples = [...unsorted].sort((a, b) => a.block - b.block);
    const { build, case_id: id, variant } = samples[0];
    const a = groups.get(measurementId(build, id, "tessera"))!;
    const b = groups.get(measurementId(build, id, "tessera-aa"))!;
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
    );
    if (variant === "tessera-aa") {
      paired.ratios = samples.map((sample) => time(sample) / first.get(sample.block)!);
      paired.ratio = median(paired.ratios);
      paired.low = Math.min(...paired.ratios);
      paired.high = Math.max(...paired.ratios);
    }
    if (variant.startsWith("tessera")) paired.verdict = "within noise";
    result.set(measurementId(build, id, variant), {
      caseId: id,
      build,
      variant,
      stable:
        variant.startsWith("tessera") ||
        dataset.manifests.get(build)!.variants.find((v) => v.name === variant)!.stable,
      ...paired,
      ns: median(samples.map((s) => time(s) / Number(id.split("/")[2]))),
    });
  }
  return result;
}
