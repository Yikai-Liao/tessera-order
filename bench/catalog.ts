// SPDX-License-Identifier: BSD-2-Clause

import { readFileSync } from "node:fs";
import { z } from "zod";
import { decodeConfig } from "./config.ts";

const identity = z.object({ id: z.string(), label: z.string() });
const inventory = z.object({
  types: z.array(z.string()),
  shapes: z.array(z.string()),
  lengths: z.array(z.number().int().positive()),
});

/** Language-neutral benchmark identities, presentation metadata and suite order. */
export const catalog = z
  .object({
    schema: z.literal(1),
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
    configurations: z.array(
      z.object({
        id: z.string(),
        name: z.string(),
        testbed: z.string(),
        relativeTestbed: z.string(),
        features: z.array(z.string()),
      }),
    ),
    smoke: inventory,
    comparison: inventory.extend({
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

/** Continuous configurations and their shared publication identity. */
export const VARIANTS = catalog.configurations;
export type Variant = (typeof VARIANTS)[number];

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

/** Format a case's readable label while preserving unknown identifiers. */
export function caseLabel(id: string) {
  const parts = caseParts(id);
  return (
    [parts.type, parts.input, parts.n].filter(Boolean).join(" · ") +
    (parts.extra ? `, ${parts.extra}` : "")
  );
}
