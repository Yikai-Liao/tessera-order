// SPDX-License-Identifier: BSD-2-Clause

import { readFileSync } from "node:fs";
import { z } from "zod";
import { decodeConfig } from "./config.ts";

const identity = z.object({ id: z.string(), label: z.string() });
const inventory = z.object({
  types: z.array(z.string()),
  shapes: z.array(z.string()),
  lengths: z.array(z.number().int().positive()),
  modes: z.array(z.string()),
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
        features: z.array(z.string()),
      }),
    ),
    smoke: inventory,
    comparison: inventory.extend({
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
