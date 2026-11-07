// SPDX-License-Identifier: BSD-2-Clause

export const shapes = [
  "random",
  "d256",
  "d16",
  "zipf1.1",
  "zipf2",
  "skew99",
  "sorted",
  "reverse",
  "sorted99",
  "runs",
];
export const types = ["u32", "u64", "f64", "row_u64", "record128", "owned", "row_u32"];
export const lengths = [1000, 65536, 1000000, 10000000];
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
  dispatch?: Record<string, string>;
  [key: string]: unknown;
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
export function caseId(c: Case) {
  return `${c.ty}/${c.distribution}/${c.len}/s1`;
}
export function median(values: number[]) {
  if (!values.length) throw new Error("cannot summarize an empty sample");
  const sorted = [...values].sort((a, b) => a - b);
  const mid = sorted.length >> 1;
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}
export function geomean(values: number[]) {
  if (!values.length || values.some((v) => !Number.isFinite(v) || v <= 0))
    throw new Error("geometric mean requires positive values");
  return Math.exp(values.reduce((a, b) => a + Math.log(b), 0) / values.length);
}
export function key(build: string, id: string, variant: string) {
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
export function summarize(data: Dataset): Map<string, Summary> {
  const groups = Map.groupBy(data.samples, (s) => key(s.build, s.case_id, s.variant));
  const result = new Map<string, Summary>();
  for (const unsorted of groups.values()) {
    const samples = [...unsorted].sort((a, b) => a.block - b.block);
    const { build, case_id: id, variant } = samples[0];
    const a = groups.get(key(build, id, "tessera"))!;
    const b = groups.get(key(build, id, "tessera-aa"))!;
    const time = (s: Sample) => s.elapsed_ns / s.batch;
    const times = (values: Sample[]) => new Map(values.map((s) => [s.block, time(s)]));
    const first = times(a),
      second = times(b);
    const aa = a.map((s) => second.get(s.block)! / time(s));
    const ratios = samples.map((s) =>
      variant === "tessera-aa"
        ? time(s) / first.get(s.block)!
        : time(s) / ((first.get(s.block)! + second.get(s.block)!) / 2),
    );
    const ratio = median(ratios),
      aaLow = Math.min(...aa),
      aaHigh = Math.max(...aa);
    const verdict =
      variant === "tessera" || variant === "tessera-aa"
        ? "within noise"
        : ratio < aaLow && ratios.filter((r) => r < 1).length >= 8
          ? "faster"
          : ratio > aaHigh && ratios.filter((r) => r > 1).length >= 8
            ? "slower"
            : "within noise";
    result.set(key(build, id, variant), {
      caseId: id,
      build,
      variant,
      stable:
        variant.startsWith("tessera") ||
        data.manifests.get(build)!.variants.find((v) => v.name === variant)!.stable,
      ratios,
      ratio,
      low: Math.min(...ratios),
      high: Math.max(...ratios),
      aaLow,
      aaHigh,
      ns: median(samples.map((s) => time(s) / Number(id.split("/")[2]))),
      verdict,
    });
  }
  return result;
}
