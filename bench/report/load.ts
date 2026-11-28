// SPDX-License-Identifier: BSD-2-Clause

import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";
import { isDeepStrictEqual } from "node:util";
import {
  caseGroup,
  columnId,
  isCollection,
  operationKey,
  operationSchema,
  parseColumnId,
  referenceColumn,
  validateOperation,
} from "../catalog.ts";
import { parsePlan, planOperations } from "../plan.ts";
import { pairedBudget } from "../statistics.ts";
import {
  caseId,
  type Dataset,
  type Manifest,
  type Memory,
  measurementId,
  type Observation,
  pairedColumns,
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
  if (value.schema !== 2)
    throw new Error(
      "unsupported measurement schema; regenerate with the tooling revision that wrote it",
    );
  if (!["smoke", "release", "custom"].includes(String(value.plan_name)))
    throw new Error("unsupported measurement plan");
  assertInteger(value.blocks, 1);
  pairedBudget(value.blocks);
  if (value.build !== "default")
    throw new Error("Only the current single-build format is supported");
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
  if (!Array.isArray(value.cases) || !Array.isArray(value.columns))
    throw new Error("missing case or column plan");
  const plan = parsePlan(value.plan);
  const operations = planOperations(plan);
  if (operations.length !== 1) throw new Error("a measurement session has exactly one operation");
  const [operation] = operations;
  const collection = isCollection(operation);
  const { control } = pairedColumns(operation);
  if (operation.kind !== "sort" && !collection) assertInteger(value.index_bytes, 1);
  const reference = columnId(referenceColumn(operation, "serial")!);
  const columns = new Set<string>();
  for (const column of value.columns) {
    assertObject(column);
    assertString(column.id);
    const parsed = parseColumnId(column.id);
    if (
      column.implementation !== parsed.implementation ||
      column.execution !== parsed.execution ||
      typeof column.stable !== "boolean" ||
      columns.has(column.id)
    )
      throw new Error("invalid column plan");
    assertString(column.adapter);
    columns.add(column.id);
  }
  const ids = new Set<string>();
  for (const c of value.cases) {
    assertObject(c);
    assertString(c.ty);
    assertString(c.distribution);
    assertInteger(c.len, 1);
    const actual = operationSchema.parse(c.operation);
    validateOperation(actual, c.len);
    if (
      actual.kind !== operation.kind ||
      (collection && operationKey(actual) !== operationKey(operation))
    )
      throw new Error("case operation differs from its plan");
    if (collection) {
      assertInteger(c.work_items, 1);
      assertInteger(c.input_entries, c.len);
      assertInteger(c.cold_queries);
      assertInteger(c.churn_pairs);
    }
    if (
      !Array.isArray(c.columns) ||
      !c.columns.includes(control) ||
      !c.columns.includes(reference) ||
      new Set(c.columns).size !== c.columns.length ||
      c.columns.some((id) => !columns.has(id))
    )
      throw new Error("invalid case columns");
    if (!Array.isArray(c.omitted)) throw new Error("missing unsupported-column evidence");
    const omitted = c.omitted.map((entry) => {
      assertObject(entry);
      assertString(entry.column);
      assertString(entry.reason);
      return entry.column;
    });
    // Each planned execution of a case pairs every available planned implementation; presets
    // drop implementations this build lacks.
    const planned = [...(c.columns as string[]), ...omitted].map(parseColumnId);
    const names = new Set(planned.map((column) => column.implementation));
    const executions = new Set(planned.map((column) => column.execution));
    const expected = [...executions].flatMap((execution) =>
      [...names].map((name) => columnId({ implementation: name, execution })),
    );
    if (
      [...names].some((name) => !plan.implementations.includes(name)) ||
      !isDeepStrictEqual(planned.map(columnId).toSorted(), expected.toSorted())
    )
      throw new Error("incomplete unsupported-column evidence");
    assertObject(c.routes);
    for (const route of Object.values(c.routes)) assertString(route);
    if (!isDeepStrictEqual(Object.keys(c.routes).sort(), [...(c.columns as string[])].sort()))
      throw new Error("missing implementation route");
    const id = caseId(c as unknown as Manifest["cases"][number]);
    if (ids.has(id)) throw new Error(`duplicate case ${id}`);
    ids.add(id);
  }
  return value as unknown as Manifest;
}
/** Sort memory beyond a million wide records is measured only for Tessera's columns. */
function requiresMemory(c: Manifest["cases"][number], column: string) {
  if (isCollection(c.operation))
    return c.operation.kind === "map-build" || c.operation.kind === "map-mixed";
  if (c.operation.kind !== "sort" || c.len <= 1000000 || c.ty === "u64") return true;
  return parseColumnId(column).implementation === "tessera";
}
export function validateRun(dataset: Dataset): void {
  const m = parseManifest(dataset.manifest);
  pairedBudget(m.blocks);
  const { control, repeat } = pairedColumns(m.cases[0].operation);
  const build = "default";
  const samples = new Map<string, Sample[]>(),
    identities = new Set<string>(),
    slots = new Set<string>();
  for (const s of dataset.samples) {
    assertObject(s);
    assertString(s.case_id);
    assertString(s.column);
    assertString(s.build);
    assertString(s.group);
    assertInteger(s.block);
    assertInteger(s.slot);
    assertInteger(s.batch, 1);
    assertInteger(s.elapsed_ns, 1);
    const c = m.cases.find((c) => caseId(c) === s.case_id);
    if (
      s.build !== build ||
      !c ||
      s.block >= m.blocks ||
      s.slot >= c.columns.length + 1 ||
      s.verified !== true ||
      ![...c.columns, repeat].includes(s.column) ||
      s.group !== caseGroup(c)
    )
      throw new Error(`invalid or unverified sample ${s.case_id}/${s.column}`);
    const id = `${measurementId(s.build, s.case_id, s.column)}:${s.block}`,
      slot = `${s.build}:${s.case_id}:${s.block}:${s.slot}`;
    if (identities.has(id) || slots.has(slot)) throw new Error(`duplicate sample or slot ${id}`);
    identities.add(id);
    slots.add(slot);
    const k = measurementId(s.build, s.case_id, s.column);
    if (!samples.has(k)) samples.set(k, []);
    samples.get(k)!.push(s);
  }
  const observations = new Map<string, Observation>();
  for (const o of dataset.observations) {
    assertObject(o);
    assertString(o.group);
    assertString(o.build);
    assertInteger(o.unix_seconds);
    if (!["start", "end"].includes(o.phase) || o.build !== build)
      throw new Error("invalid host observation");
    const id = `${o.build}:${o.group}:${o.phase}`;
    if (observations.has(id)) throw new Error(`duplicate observation ${id}`);
    observations.set(id, o);
  }
  for (const c of m.cases) {
    const has = [...c.columns, repeat].map((column) =>
      samples.get(measurementId(build, caseId(c), column)),
    );
    const group = `${build}:${caseGroup(c)}`;
    if (
      dataset.partial &&
      has.every((s) => !s) &&
      !observations.has(`${group}:start`) &&
      !observations.has(`${group}:end`)
    )
      continue;
    if (has.some((s) => s?.length !== m.blocks))
      throw new Error(`missing blocks: ${build}/${caseId(c)}`);
    const start = observations.get(`${group}:start`),
      end = observations.get(`${group}:end`);
    if (!start || !end || end.unix_seconds < start.unix_seconds)
      throw new Error(`incomplete group ${group}`);
  }
  if (!dataset.samples.length) throw new Error("no measurements");
  const memories = new Set<string>();
  for (const r of dataset.memory) {
    assertObject(r);
    assertInteger(r.peak_extra_bytes);

    assertString(r.case_id);
    assertString(r.column);
    assertString(r.build);
    const id = measurementId(r.build, r.case_id, r.column);
    const c = m.cases.find((c) => caseId(c) === r.case_id);
    if (
      r.build !== build ||
      r.verified !== true ||
      !c?.columns.includes(r.column) ||
      memories.has(id)
    )
      throw new Error(`invalid or duplicate memory sample ${id}`);
    if (isCollection(c.operation)) {
      assertInteger(r.retained_bytes);
      if (
        !requiresMemory(c, r.column) ||
        r.live_entries !== c.len ||
        r.retained_bytes! > r.peak_extra_bytes
      )
        throw new Error("invalid retained collection heap");
    } else if (c.operation.kind !== "sort") {
      assertInteger(r.output_bytes);
      const expected =
        c.operation.kind === "argsort"
          ? c.len * m.index_bytes!
          : c.operation.kind === "partition-buckets"
            ? (c.operation.bucket_count + 1) * m.index_bytes!
            : 0;
      if (r.output_bytes !== expected || r.peak_extra_bytes < r.output_bytes)
        throw new Error(
          "mandatory output allocation differs from the operation or exceeds its peak",
        );
    }
    memories.add(id);
  }
  for (const c of m.cases) {
    if (dataset.partial && !samples.has(measurementId(build, caseId(c), control))) continue;
    for (const column of c.columns)
      if (requiresMemory(c, column) && !memories.has(measurementId(build, caseId(c), column)))
        throw new Error(`missing memory sample ${build}/${caseId(c)}/${column}`);
  }
}
/** Load current single-build runs, checking provenance before merging their journals. */
export async function loadRun(directories: string[], partial = false): Promise<Dataset> {
  let dataset: Dataset | undefined;
  for (const directory of directories) {
    if ((await readdir(directory)).some((name) => name.startsWith("manifest-")))
      throw new Error(
        "Multiple-build artifacts are unsupported; use their original revision to regenerate",
      );
    const host = await readJson(join(directory, "host.json"));
    assertObject(host);
    const manifest = parseManifest(await readJson(join(directory, "manifest.json")));
    if (dataset) {
      if (!isDeepStrictEqual(host, dataset.host))
        throw new Error("host fingerprints differ; cannot merge runs");
      if (!isDeepStrictEqual(manifest, dataset.manifest))
        throw new Error("build provenance differs");
    } else {
      dataset = { host, manifest, samples: [], observations: [], memory: [], partial };
    }
    const memory = await readJson(join(directory, "memory-manifest-default.json"));
    assertObject(memory);
    for (const field of [
      ...(manifest.index_bytes === undefined ? [] : ["plan", "cases", "index_bytes"]),
      "schema",
      "blocks",
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
      if (!isDeepStrictEqual(memory[field], manifest[field]))
        throw new Error(`memory provenance differs: default/${field}`);
    }
    for (const row of await readJsonLines(join(directory, "blocks.jsonl"))) {
      assertObject(row);
      if (row.kind === "sample") dataset.samples.push(row as unknown as Sample);
      else if (row.kind === "observation") dataset.observations.push(row as unknown as Observation);
      else throw new Error("unknown block record");
    }
    dataset.memory.push(...((await readJsonLines(join(directory, "memory.jsonl"))) as Memory[]));
  }
  if (!dataset) throw new Error("No result directories");
  validateRun(dataset);
  return dataset;
}
