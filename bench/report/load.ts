// SPDX-License-Identifier: BSD-2-Clause

import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { isDeepStrictEqual } from "node:util";
import {
  caseId,
  type Dataset,
  type Manifest,
  type Memory,
  measurementId,
  type Observation,
  type Sample,
} from "./model.ts";

async function readJson(path: string): Promise<unknown> {
  return JSON.parse(await readFile(path, "utf8"));
}
async function readJsonLines(path: string): Promise<unknown[]> {
  const text = await readFile(path, "utf8");
  if (text && !text.endsWith("\n")) throw new Error(`${path}: incomplete final line`);
  return text.trim()
    ? text
        .trimEnd()
        .split("\n")
        .map((line, i) => {
          try {
            return JSON.parse(line);
          } catch {
            throw new Error(`${path}:${i + 1}: invalid JSON`);
          }
        })
    : [];
}
function assertObject(value: unknown): asserts value is Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new Error("expected a JSON object");
}
function assertInteger(value: unknown, min = 0): asserts value is number {
  if (!Number.isSafeInteger(value) || Number(value) < min)
    throw new Error(`invalid integer: ${value}`);
}
function assertString(value: unknown): asserts value is string {
  if (typeof value !== "string" || !value.length) throw new Error("expected a nonempty string");
}
function parseManifest(value: unknown): Manifest {
  assertObject(value);
  if (
    value.schema !== 1 ||
    value.suite_version !== 1 ||
    value.blocks !== 9 ||
    !["smoke", "release"].includes(String(value.suite))
  )
    throw new Error("unsupported measurement schema or suite");
  if (!["default", "portable-simd"].includes(String(value.build))) throw new Error("unknown build");
  for (const name of [
    "commit",
    "rustc",
    "source_sha256",
    "binary_sha256",
    "cargo_lock",
    "target",
    "profile",
    "opt_level",
    "debug",
  ])
    assertString(value[name]);
  if (value.native_build !== undefined) {
    assertString(value.cc);
    assertString(value.cxx);
  }
  if (!Array.isArray(value.cases) || !Array.isArray(value.variants))
    throw new Error("missing case or variant plan");
  const names = new Set<string>();
  for (const v of value.variants) {
    assertObject(v);
    assertString(v.name);
    if (typeof v.stable !== "boolean" || names.has(v.name)) throw new Error("invalid variant plan");
    names.add(v.name);
  }
  const ids = new Set<string>();
  for (const c of value.cases) {
    assertObject(c);
    assertString(c.ty);
    assertString(c.distribution);
    assertInteger(c.len, 1);
    if (
      !Array.isArray(c.variants) ||
      c.variants.length < 2 ||
      !c.variants.includes("tessera") ||
      !c.variants.includes("std-stable") ||
      new Set(c.variants).size !== c.variants.length ||
      c.variants.some((v) => !names.has(v))
    )
      throw new Error("invalid case capabilities");
    const id = caseId(c as unknown as Manifest["cases"][number]);
    if (ids.has(id)) throw new Error(`duplicate case ${id}`);
    ids.add(id);
  }
  return value as unknown as Manifest;
}
export function validateRun(dataset: Dataset): void {
  if (!dataset.manifests.has("default")) throw new Error("default build is missing");
  const samples = new Map<string, Sample[]>(),
    identities = new Set<string>(),
    slots = new Set<string>();
  for (const s of dataset.samples) {
    assertObject(s);
    assertString(s.case_id);
    assertString(s.variant);
    assertString(s.build);
    assertString(s.group);
    assertInteger(s.block);
    assertInteger(s.slot);
    assertInteger(s.batch, 1);
    assertInteger(s.elapsed_ns, 1);
    const m = dataset.manifests.get(s.build),
      c = m?.cases.find((c) => caseId(c) === s.case_id);
    if (
      !c ||
      s.block >= 9 ||
      s.slot >= c.variants.length + 1 ||
      s.verified !== true ||
      ![...c.variants, "tessera-aa"].includes(s.variant) ||
      s.group !== `${c.ty}/${c.distribution}`
    )
      throw new Error(`invalid or unverified sample ${s.case_id}/${s.variant}`);
    const id = `${measurementId(s.build, s.case_id, s.variant)}:${s.block}`,
      slot = `${s.build}:${s.case_id}:${s.block}:${s.slot}`;
    if (identities.has(id) || slots.has(slot)) throw new Error(`duplicate sample or slot ${id}`);
    identities.add(id);
    slots.add(slot);
    const k = measurementId(s.build, s.case_id, s.variant);
    if (!samples.has(k)) samples.set(k, []);
    samples.get(k)!.push(s);
  }
  const observations = new Map<string, Observation>();
  for (const o of dataset.observations) {
    assertObject(o);
    assertString(o.group);
    assertString(o.build);
    assertInteger(o.unix_seconds);
    if (!["start", "end"].includes(o.phase) || !dataset.manifests.has(o.build))
      throw new Error("invalid host observation");
    const id = `${o.build}:${o.group}:${o.phase}`;
    if (observations.has(id)) throw new Error(`duplicate observation ${id}`);
    observations.set(id, o);
  }
  for (const [build, m] of dataset.manifests) {
    for (const c of m.cases) {
      const has = [...c.variants, "tessera-aa"].map((v) =>
        samples.get(measurementId(build, caseId(c), v)),
      );
      const group = `${build}:${c.ty}/${c.distribution}`;
      if (
        dataset.partial &&
        has.every((s) => !s) &&
        !observations.has(`${group}:start`) &&
        !observations.has(`${group}:end`)
      )
        continue;
      if (has.some((s) => s?.length !== 9))
        throw new Error(`missing blocks: ${build}/${caseId(c)}`);
      const start = observations.get(`${group}:start`),
        end = observations.get(`${group}:end`);
      if (!start || !end || end.unix_seconds < start.unix_seconds)
        throw new Error(`incomplete group ${group}`);
    }
  }
  if (!dataset.samples.length) throw new Error("no measurements");
  const memories = new Set<string>();
  for (const r of dataset.memory) {
    assertObject(r);
    assertInteger(r.peak_extra_bytes);
    assertString(r.case_id);
    assertString(r.variant);
    assertString(r.build);
    const id = measurementId(r.build, r.case_id, r.variant);
    const c = dataset.manifests.get(r.build)?.cases.find((c) => caseId(c) === r.case_id);
    if (r.verified !== true || !c?.variants.includes(r.variant) || memories.has(id))
      throw new Error(`invalid or duplicate memory sample ${id}`);
    memories.add(id);
  }
  for (const [build, m] of dataset.manifests) {
    for (const c of m.cases) {
      if (dataset.partial && !samples.has(measurementId(build, caseId(c), "tessera"))) continue;
      for (const v of c.variants) {
        if (
          (c.len <= 1000000 || c.ty === "u64" || v === "tessera") &&
          !memories.has(measurementId(build, caseId(c), v))
        )
          throw new Error(`missing memory sample ${build}/${caseId(c)}/${v}`);
      }
    }
  }
}
export async function loadRun(directories: string[], partial = false): Promise<Dataset> {
  const dataset: Dataset = {
    host: {},
    manifests: new Map(),
    samples: [],
    observations: [],
    memory: [],
    partial,
  };
  let first = true;
  for (const directory of directories) {
    const host = await readJson(join(directory, "host.json"));
    assertObject(host);
    if (!first && !isDeepStrictEqual(host, dataset.host))
      throw new Error("host fingerprints differ; cannot merge runs");
    dataset.host = host;
    first = false;
    for (const build of ["default", "portable-simd"]) {
      let value: unknown;
      try {
        value = await readJson(
          join(directory, build === "default" ? "manifest.json" : `manifest-${build}.json`),
        );
      } catch (e) {
        if ((e as NodeJS.ErrnoException).code === "ENOENT") continue;
        throw e;
      }
      const m = parseManifest(value);
      if (m.build !== build) throw new Error("manifest build disagrees with file name");
      if (dataset.manifests.has(build) && !isDeepStrictEqual(m, dataset.manifests.get(build)))
        throw new Error(`build provenance differs: ${build}`);
      const memory = await readJson(join(directory, `memory-manifest-${build}.json`));
      assertObject(memory);
      for (const field of [
        "schema",
        "source_sha256",
        "rustc",
        "target",
        "profile",
        "opt_level",
        "debug",
        "features",
        "cargo_encoded_rustflags",
        "cc",
        "cxx",
        "native_build",
        "native_sources",
      ]) {
        if (!isDeepStrictEqual(memory[field], m[field]))
          throw new Error(`memory provenance differs: ${build}/${field}`);
      }
      dataset.manifests.set(build, m);
    }
    for (const row of await readJsonLines(join(directory, "blocks.jsonl"))) {
      assertObject(row);
      if (row.kind === "sample") dataset.samples.push(row as unknown as Sample);
      else if (row.kind === "observation") dataset.observations.push(row as unknown as Observation);
      else throw new Error("unknown block record");
    }
    dataset.memory.push(...((await readJsonLines(join(directory, "memory.jsonl"))) as Memory[]));
  }
  const baseline = dataset.manifests.get("default"),
    simd = dataset.manifests.get("portable-simd");
  if (baseline && simd) {
    for (const field of [
      "commit",
      "source_sha256",
      "rustc",
      "target",
      "profile",
      "opt_level",
      "debug",
      "cargo_encoded_rustflags",
      "cargo_lock",
    ]) {
      if (!isDeepStrictEqual(baseline[field], simd[field]))
        throw new Error(`builds do not share a source and compiler configuration: ${field}`);
    }
  }
  validateRun(dataset);
  return dataset;
}
