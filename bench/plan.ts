// SPDX-License-Identifier: BSD-2-Clause

import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { z } from "zod";
import { catalog, type Operation, operationSchema, validateOperation } from "./catalog.ts";
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
});
const schema = z
  .strictObject({
    $comment: z.string().optional(),
    schema: z.union([z.literal(1), z.literal(2)]),
    operation: operationSchema.optional(),
    algorithms: choices(ids(catalog.algorithms)),
    include: z.array(rule).min(1),
    exclude: z.array(rule).default([]),
    parallel: z.array(rule).optional(),
  })
  .superRefine((plan, ctx) => {
    if (
      plan.schema === 1
        ? plan.operation !== undefined
        : !plan.operation || plan.operation.kind === "sort"
    )
      ctx.addIssue({
        code: "custom",
        message: "schema 2 requires an explicit operation; schema 1 is sorting only",
      });
  });

/** A product of optional axes; omitted axes match every supported value. */
export type CaseRule = z.infer<typeof rule>;
/** A validated experiment selection, independent of its measurement protocol. */
export type Plan = z.infer<typeof schema>;
export type SelectionOptions = CaseRule & {
  algorithms?: string[];
  operation?: Operation["kind"];
  prefixLen?: number;
  k?: number;
  index?: number;
  rankRange?: { start: number; end: number };
  indices?: number[];
  segmentLen?: number;
  bucketCount?: number;
};

/** Validate selections without choosing a measurement protocol or adding algorithms. */
export function parsePlan(value: unknown): Plan {
  const plan = schema.parse(value);
  if (plan.operation && plan.operation.kind !== "sort") {
    // New operations use exact parameters and literal counts in this first slice.
    // Engine-relative boundaries remain resolved by Rust for sorting plans.
    const rules = [...plan.include, ...plan.exclude, ...(plan.parallel ?? [])];
    if (rules.some((r) => r.lengths?.some((n) => typeof n !== "number")))
      throw new Error("Non-sort plans require literal record counts");
    for (const rule of plan.include.concat(plan.parallel ?? [])) {
      for (const len of rule.lengths ?? catalog.inputs.lengths) {
        const includedTypes = rule.types ?? catalog.types.map((t) => t.id);
        const includedShapes = rule.distributions ?? catalog.distributions.map((d) => d.id);
        const survives = includedTypes.some((ty) =>
          includedShapes.some(
            (distribution) =>
              !plan.exclude.some(
                (r) =>
                  (!r.types || r.types.includes(ty)) &&
                  (!r.distributions || r.distributions.includes(distribution)) &&
                  (!r.lengths || r.lengths.includes(len)),
              ),
          ),
        );
        if (survives) validateOperation(plan.operation, len as number);
      }
    }
  }
  return plan;
}

/** Load a TOML preset or a JSON/TOML file, then apply explicit axis overrides. */
export async function loadPlan(name: string, options: SelectionOptions = {}): Promise<Plan> {
  const path = ["local", "continuous", "smoke", "release", "parallel", "growth"].includes(name)
    ? fileURLToPath(new URL(`./plans/${name}.toml`, import.meta.url))
    : name;
  const plan = schema.parse(decodeConfig(await readFile(path, "utf8"), path));
  const {
    algorithms,
    operation,
    prefixLen,
    k,
    index,
    rankRange,
    indices,
    segmentLen,
    bucketCount,
    ...axes
  } = options;
  const parameters = Object.fromEntries(
    Object.entries({
      prefix_len: prefixLen,
      k,
      index,
      ...rankRange,
      indices,
      segment_len: segmentLen,
      bucket_count: bucketCount,
    }).filter(([, value]) => value !== undefined),
  );
  let selected = plan;
  if (operation !== undefined || Object.keys(parameters).length) {
    const kind = operation ?? plan.operation?.kind;
    if (!kind) throw new Error("Operation parameters require --operation");
    const previous = plan.operation?.kind === kind ? plan.operation : { kind };
    const requested = operationSchema.parse({ ...previous, ...parameters });
    const { operation: _previous, ...rest } = plan;
    selected =
      requested.kind === "sort"
        ? { ...rest, schema: 1 }
        : { ...rest, schema: 2, operation: requested };
  }
  return parsePlan({
    ...selected,
    algorithms: algorithms ?? plan.algorithms,
    include: plan.include.map((entry) => ({ ...entry, ...axes })),
    ...(plan.parallel ? { parallel: plan.parallel.map((entry) => ({ ...entry, ...axes })) } : {}),
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
