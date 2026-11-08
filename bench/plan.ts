// SPDX-License-Identifier: BSD-2-Clause

import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { z } from "zod";
import { catalog } from "./catalog.ts";
import { decodeConfig } from "./config.ts";

const ids = (entries: { id: string }[]) => z.enum(entries.map((entry) => entry.id));
const choices = <T extends z.ZodType>(schema: T) =>
  z
    .array(schema)
    .min(1)
    .refine((values) => new Set(values).size === values.length, "duplicate choices");
const rule = z.strictObject({
  types: choices(ids(catalog.types)).optional(),
  distributions: choices(ids(catalog.distributions)).optional(),
  lengths: choices(
    z.union([
      z.number().int().positive().max(Number.MAX_SAFE_INTEGER),
      z.enum(["insertion-max", "insertion-next", "tile-next"]),
    ]),
  ).optional(),
  modes: choices(ids(catalog.modes)).optional(),
});
const schema = z.strictObject({
  $comment: z.string().optional(),
  schema: z.literal(1),
  harness: z.enum(["smoke", "comparison"]),
  algorithms: choices(ids(catalog.algorithms)),
  include: z.array(rule).min(1),
  exclude: z.array(rule).default([]),
});

/** A product of optional axes; omitted axes match every supported value. */
export type CaseRule = z.infer<typeof rule>;
/** A validated experiment selection, independent of its measurement protocol. */
export type Plan = z.infer<typeof schema>;
export type SelectionOptions = CaseRule & { algorithms?: string[] };

/** Validate selectors and retain the controls required by the chosen harness. */
export function parsePlan(value: unknown): Plan {
  const plan = schema.parse(value);
  const controls = plan.harness === "comparison" ? ["tessera", "std-stable"] : ["tessera"];
  if (plan.harness === "smoke" && plan.algorithms.some((id) => id !== "tessera"))
    throw new Error(
      "The smoke harness measures Tessera only; use a comparison plan for algorithms",
    );
  if (
    plan.harness === "comparison" &&
    [...plan.include, ...plan.exclude].some((entry) => entry.modes?.some((mode) => mode !== "api"))
  )
    throw new Error("The comparison harness supports api mode only");
  const inventory = plan.harness === "smoke" ? catalog.smoke : catalog.comparison;
  for (const entry of [...plan.include, ...plan.exclude]) {
    if (
      entry.types?.some((id) => !inventory.types.includes(id)) ||
      entry.distributions?.some((id) => !inventory.shapes.includes(id)) ||
      entry.modes?.some((id) => !inventory.modes.includes(id))
    )
      throw new Error(`Selector is unsupported by the ${plan.harness} harness`);
  }
  plan.algorithms = [...new Set([...controls, ...plan.algorithms])];
  return plan;
}

/** Load a TOML preset or a JSON/TOML file, then apply explicit axis overrides. */
export async function loadPlan(name: string, options: SelectionOptions = {}): Promise<Plan> {
  const path = ["local", "continuous", "smoke", "release"].includes(name)
    ? fileURLToPath(new URL(`./plans/${name}.toml`, import.meta.url))
    : name;
  const plan = schema.parse(decodeConfig(await readFile(path, "utf8"), path));
  const { algorithms, ...axes } = options;
  return parsePlan({
    ...plan,
    algorithms: algorithms ?? plan.algorithms,
    include: plan.include.map((entry) => ({ ...entry, ...axes })),
  });
}

/** Export the same validation rules for editor completion and external tooling. */
export function planSchema() {
  return z.toJSONSchema(schema);
}

/** Apply an optional CLI pattern to IDs resolved by the native harness. */
export function selectCaseIds(ids: string[], pattern?: string) {
  const filter = pattern === undefined ? undefined : new RegExp(pattern);
  const selected = ids.filter((id) => !filter || filter.test(id));
  if (!selected.length) throw new Error("The benchmark plan selects no supported cases");
  return selected;
}
