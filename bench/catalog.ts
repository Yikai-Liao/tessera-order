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
        tracking_path: z
          .string()
          .regex(/^[a-z0-9]+(?:-[a-z0-9]+)*$/)
          .optional(),
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

/** Identity fields shared by input manifests and the two benchmark ID grammars. */
export interface CaseIdentity {
  ty: string;
  distribution: string;
  len: number;
}

/** Format the comparison harness's seed-one input identity. */
export function comparisonId(input: CaseIdentity) {
  return `${input.ty}/${input.distribution}/${input.len}/s1`;
}

/** Format a Criterion timed identity; catalog membership is checked by its consumer. */
export function criterionId(input: CaseIdentity, mode: string) {
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
    parts.length !== 4 ||
    !ty ||
    !distribution ||
    !/^[1-9]\d*$/.test(length) ||
    !Number.isSafeInteger(Number(length)) ||
    !/^s[1-9]\d*$/.test(seed)
  )
    throw new Error(`Invalid comparison benchmark ID: ${id}`);
  return { ty, distribution, len: Number(length), seed };
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

export const inventorySchema = z
  .object({ supported: z.array(z.string()), unsupported: z.array(z.string()) })
  .superRefine(({ supported, unsupported }, ctx) => {
    const all = [...supported, ...unsupported];
    if (!supported.length || new Set(all).size !== all.length)
      ctx.addIssue({ code: "custom", message: "Empty or overlapping tracking inventory" });
    for (const id of all) {
      try {
        parseCriterionId(id);
      } catch {
        ctx.addIssue({ code: "custom", message: "Invalid tracking case: " + id });
      }
    }
  });
export type BenchmarkInventory = z.infer<typeof inventorySchema>;

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
