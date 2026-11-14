// SPDX-License-Identifier: BSD-2-Clause

import { readFileSync } from "node:fs";
import { z } from "zod";
import { decodeConfig } from "./config.ts";

const identity = z.object({ id: z.string(), label: z.string() });
const blockBudget = z.object({
  count: z.number().int().positive(),
  agreement: z.number().int().positive(),
});
const inputDefaults = z.object({ lengths: z.array(z.number().int().positive()) });

/** Language-neutral benchmark identities, presentation metadata and suite order. */
export const catalog = z
  .object({
    schema: z.literal(1),
    pairedBlocks: z.object({ short: blockBudget, extended: blockBudget }),
    revisionSampling: z
      .array(
        z.object({
          min_len: z.number().int().positive(),
          warm_ms: z.number().int().positive(),
          window_ms: z.number().int().positive(),
          samples: z.number().int().min(10).max(100),
        }),
      )
      .min(1),
    algorithms: z.array(
      identity.extend({
        stable: z.boolean(),
        family: z.string(),
        adapter: z.string(),
        threads: z.number().int().positive().optional(),
        detail: z.string().optional(),
        description: z.string().optional(),
      }),
    ),
    types: z.array(
      identity.extend({
        detail: z.string(),
        task: z.string(),
        bytes: z.number().int().positive(),
        compact: z.string().optional(),
      }),
    ),
    distributions: z.array(
      identity.extend({
        compact: z.string().optional(),
        group: z.string().optional(),
      }),
    ),
    modes: z.array(identity),
    continuous: z.object({ testbed: z.string(), features: z.array(z.string()) }),
    inputs: inputDefaults,
    comparison: z.object({
      types: z.array(z.string()),
      summaryShapes: z.array(z.string()),
      typeGroups: z.array(
        z.object({
          title: z.string(),
          keys: z.boolean(),
          types: z.array(z.string()),
        }),
      ),
    }),
  })
  .parse(
    decodeConfig(readFileSync(new URL("./catalog.toml", import.meta.url), "utf8"), "catalog.toml"),
  );

/** Shared revision budget consumed by the runner and trusted completeness check. */
export function revisionSampling(len: number) {
  const settings = catalog.revisionSampling.find((s) => len >= s.min_len);
  if (!settings) throw new Error("No revision sampling budget");
  return settings;
}

/** The one hosted build: its Cloud Testbed and required Cargo features. */
export const continuous = catalog.continuous;

function index<T extends { id: string }>(entries: T[]) {
  return new Map(entries.map((entry) => [entry.id, entry]));
}

const algorithms = index(catalog.algorithms);
const records = index(catalog.types);
const distributions = index(catalog.distributions);
const modes = index(catalog.modes);

/** Look up an algorithm's presentation without parsing its identifier. */
export function algorithm(id: string): (typeof catalog.algorithms)[number] {
  return (
    algorithms.get(id) ?? { id, label: id, family: "comparison", adapter: "unknown", stable: false }
  );
}

/** Look up a record representation's labels and task. */
export function recordType(id: string) {
  return records.get(id) ?? { id, label: id, compact: id, detail: "", task: "unknown" };
}

/** Look up an input distribution's labels and presentation group. */
export function distribution(id: string) {
  return distributions.get(id) ?? { id, label: id, compact: id, group: "" };
}

/** Name an API mode, retaining unknown IDs from historical reports. */
export function modeName(id: string) {
  return modes.get(id)?.label ?? id;
}

/** Order known API modes before unknown ones. */
export function modeOrder(id: string) {
  const order = catalog.modes.findIndex((mode) => mode.id === id);
  return order < 0 ? catalog.modes.length : order;
}

/** Versioned public-operation parameters, separate from implementation identity. */
const count = z.number().int().min(0).max(Number.MAX_SAFE_INTEGER);
export const operationSchema = z.discriminatedUnion("kind", [
  z.strictObject({ kind: z.literal("sort") }),
  z.strictObject({ kind: z.literal("argsort") }),
  z.strictObject({ kind: z.literal("partition") }),
  z.strictObject({ kind: z.literal("partition-buckets"), bucket_count: count.min(1) }),
  z.strictObject({ kind: z.literal("segments"), segment_len: count.min(1) }),
  z.strictObject({ kind: z.literal("top-k"), k: count }),
  z.strictObject({ kind: z.literal("prefix"), prefix_len: count }),
  z.strictObject({ kind: z.literal("select"), index: count }),
  z
    .strictObject({ kind: z.literal("rank-range"), start: count, end: count })
    .refine((range) => range.start <= range.end, "reversed rank range"),
  z.strictObject({ kind: z.literal("select-many"), indices: z.array(count) }),
]);
export type Operation = z.infer<typeof operationSchema>;
/** CLI choices derive from the operation schema, never a second registry. */
export const operationKinds = operationSchema.options.map((schema) => schema.shape.kind.value);

/** Validate parameters against a resolved count before building or allocating inputs. */
export function validateOperation(operation: Operation, len: number) {
  const valid =
    operation.kind === "top-k"
      ? operation.k <= len
      : operation.kind === "prefix"
        ? operation.prefix_len <= len
        : operation.kind === "select"
          ? operation.index < len
          : operation.kind === "rank-range"
            ? operation.end <= len
            : operation.kind === "select-many"
              ? operation.indices.every((i) => i < len)
              : true;
  if (!valid) throw new Error(`Operation parameters exceed input length ${len}`);
}

/** Decode an exact half-open rank range; empty ranges are no-op requests. */
export function parseRankRange(value: string) {
  const match = /^(0|[1-9]\d*):(0|[1-9]\d*)$/.exec(value);
  if (!match) throw new Error("expected start:end with nonnegative integer ranks");
  const parsed = operationSchema.parse({
    kind: "rank-range",
    start: Number(match[1]),
    end: Number(match[2]),
  });
  if (parsed.kind !== "rank-range") throw new Error("invalid rank range");
  return { start: parsed.start, end: parsed.end };
}

/** Extra identity components; complete sorting retains historical IDs. */
export function operationSuffix(operation?: Operation): string {
  switch (operation?.kind) {
    case undefined:
    case "sort":
      return "";
    case "argsort":
      return "/argsort";
    case "partition":
      return "/partition";
    case "partition-buckets":
      return `/partition-buckets/${operation.bucket_count}`;
    case "segments":
      return `/segments/${operation.segment_len}`;
    case "top-k":
      return `/top-k/${operation.k}`;
    case "prefix":
      return `/prefix/${operation.prefix_len}`;
    case "select":
      return `/select/${operation.index}`;
    case "rank-range":
      return `/rank-range/${operation.start}-${operation.end}`;
    case "select-many":
      return `/select-many/${operation.indices.join(",") || "none"}`;
  }
}

/** Short report label; exact parameters remain in the case identity and manifest. */
export function operationLabel(operation?: Operation) {
  switch (operation?.kind) {
    case undefined:
    case "sort":
      return "Full sort";
    case "argsort":
      return "Stable argsort";
    case "partition":
      return "Stable binary partition";
    case "partition-buckets":
      return `Stable partition · ${operation.bucket_count} categories`;
    case "segments":
      return `Stable segments · ${operation.segment_len} records each`;
    case "top-k":
      return `Stable-membership top-k · ${operation.k} unordered records`;
    case "prefix":
      return `Stable prefix · ${operation.prefix_len} records`;
    case "select":
      return `Stable selection · rank ${operation.index}`;
    case "rank-range":
      return `Stable ranks [${operation.start}, ${operation.end})`;
    case "select-many":
      return `Stable multiple selection · ${operation.indices.length} requests`;
  }
}

/** Identity fields shared by input manifests and the two benchmark ID grammars. */
export interface CaseIdentity {
  ty: string;
  distribution: string;
  len: number;
  operation?: Operation;
}

/** Format the comparison harness's seed-one input identity. */
export function comparisonId(input: CaseIdentity) {
  return `${input.ty}/${input.distribution}/${input.len}/s1${operationSuffix(input.operation)}`;
}

/** Keep resumable groups separate for different result contracts. */
export function comparisonGroup(input: CaseIdentity) {
  return `${input.ty}/${input.distribution}${operationSuffix(input.operation)}`;
}

/** Format a Criterion timed identity; catalog membership is checked by its consumer. */
export function criterionId(input: CaseIdentity, mode: string) {
  if (input.operation && input.operation.kind !== "sort")
    throw new Error("Criterion tracking supports full sorting only");
  return `rust/${comparisonId(input)}/${mode}`;
}

/** Extract a timed ID's Criterion group; callers supply a structurally valid ID. */
export function criterionGroup(id: string) {
  return id.slice(0, id.lastIndexOf("/"));
}

/** Decode comparison inputs, including historical type and distribution names. */
export function parseComparisonId(id: string) {
  const parts = id.split("/");
  const [ty, distribution, length, seed] = parts;
  if (
    ![4, 5, 6].includes(parts.length) ||
    !ty ||
    !distribution ||
    !/^[1-9]\d*$/.test(length) ||
    !Number.isSafeInteger(Number(length)) ||
    !/^s[1-9]\d*$/.test(seed)
  )
    throw new Error(`Invalid comparison benchmark ID: ${id}`);
  let operation: Operation | undefined;
  if (parts.length > 4) {
    const [kind, parameter] = parts.slice(4);
    const integer = (value: string) => {
      if (!/^(0|[1-9]\d*)$/.test(value)) throw new Error(`Invalid operation parameter: ${id}`);
      return Number(value);
    };
    let candidate: unknown;
    switch (kind) {
      case "partition-buckets":
        candidate = { kind, bucket_count: integer(parameter ?? "") };
        break;
      case "partition":
      case "argsort":
        candidate = { kind };
        break;
      case "segments":
        candidate = { kind, segment_len: integer(parameter ?? "") };
        break;
      case "top-k":
        candidate = { kind, k: integer(parameter ?? "") };
        break;
      case "prefix":
        candidate = { kind, prefix_len: integer(parameter ?? "") };
        break;
      case "select":
        candidate = { kind, index: integer(parameter ?? "") };
        break;
      case "rank-range":
        candidate = { kind, ...parseRankRange((parameter ?? "").replaceAll("-", ":")) };
        break;
      case "select-many":
        candidate = {
          kind,
          indices: parameter === "none" ? [] : (parameter ?? "").split(",").map(integer),
        };
        break;
      default:
        throw new Error(`Invalid operation identity: ${id}`);
    }
    operation = operationSchema.parse(candidate);
    validateOperation(operation, Number(length));
    if (operationSuffix(operation) !== `/${parts.slice(4).join("/")}`)
      throw new Error(`Noncanonical comparison benchmark ID: ${id}`);
  }
  return { ty, distribution, len: Number(length), seed, ...(operation ? { operation } : {}) };
}

/** Strictly decode Criterion IDs with known catalog fields, not display fallbacks. */
export function parseCriterionId(id: string) {
  const [language, ty, distribution, length, seed, mode, ...extra] = id.split("/");
  if (
    language !== "rust" ||
    extra.length ||
    !records.has(ty) ||
    !distributions.has(distribution) ||
    !/^[1-9]\d*$/.test(length) ||
    !Number.isSafeInteger(Number(length)) ||
    !/^s[1-9]\d*$/.test(seed) ||
    !modes.has(mode)
  )
    throw new Error(`Invalid Criterion benchmark ID: ${id}`);
  return { ty, distribution, len: Number(length), seed, mode };
}

/** Preserve raw Criterion components for local reports, including old unknown IDs. */
export function rawCaseParts(id: string) {
  const parts = id.split("/");
  return {
    input: parts.slice(1, 3).join(" · "),
    sub: parts.slice(3).join(" · "),
  };
}

/** Decode a continuous case ID into its display fields. */
export function caseParts(id: string) {
  const parts = id.split("/");
  if (parts.length !== 6 || !/^\d+$/.test(parts[3]))
    return { type: "", input: id, n: "", extra: "", seed: "", mode: "" };
  const [, type, input, length, seed, mode] = parts;
  const record = recordType(type),
    shape = distribution(input);
  const extra = [seed === "s1" ? "" : `seed ${seed.slice(1)}`, mode === "api" ? "" : mode].filter(
    Boolean,
  );
  return {
    type: record.compact ?? record.label,
    input: shape.compact ?? shape.label,
    n: Number(length).toLocaleString("en-US"),
    extra: extra.join(", "),
    seed,
    mode,
  };
}

/** Name a case's figure row: input, then record type and length, tagging a non-default seed. */
export function caseName(id: string) {
  const parts = caseParts(id);
  return {
    input: parts.input,
    sub: [parts.type, parts.n].filter(Boolean).join(" · "),
    tag: parts.seed && parts.seed !== "s1" ? `seed ${parts.seed.slice(1)}` : "",
  };
}

/** Order case IDs for reading: mode, length, record type, input, then seed; unknown entries last. */
export function compareCases(a: string, b: string) {
  const key = (id: string) => {
    const [, ty = "", shape = "", length = "", seed = "", mode = ""] = id.split("/");
    const rank = (list: { id: string }[], value: string) => {
      const found = list.findIndex((entry) => entry.id === value);
      return found < 0 ? list.length : found;
    };
    return [
      modeOrder(mode),
      Number(length) || Number.MAX_SAFE_INTEGER,
      rank(catalog.types, ty),
      rank(catalog.distributions, shape),
      Number(seed.slice(1)) || Number.MAX_SAFE_INTEGER,
    ];
  };
  const [x, y] = [key(a), key(b)];
  return x.reduce((order, value, i) => order || value - y[i], 0) || a.localeCompare(b);
}

/** Format a case's readable label while preserving unknown identifiers. */
export function caseLabel(id: string) {
  const parts = caseParts(id);
  return (
    [parts.type, parts.input, parts.n].filter(Boolean).join(" · ") +
    (parts.extra ? `, ${parts.extra}` : "")
  );
}
