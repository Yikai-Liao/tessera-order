// SPDX-License-Identifier: BSD-2-Clause

import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { z } from "zod";
import {
  catalog,
  compareExecutions,
  type Execution,
  type Operation,
  operationSchema,
  parseExecution,
  SORT,
  validateOperation,
} from "./catalog.ts";
import { decodeConfig } from "./config.ts";

const ids = (entries: { id: string }[]) => z.enum(entries.map((entry) => entry.id));
const choices = <T extends z.ZodType>(schema: T) =>
  z
    .array(schema)
    .min(1)
    .refine((values) => new Set(values).size === values.length, "duplicate choices");
const execution = z
  .union([z.literal("serial"), z.templateLiteral(["t", z.number().int().positive()])])
  .refine((text) => {
    try {
      return parseExecution(text) === text;
    } catch {
      return false;
    }
  }, "expected serial or tN with N >= 1");
/** A product of case and execution axes. Absent include axes use the catalog,
 * `sort` and `serial`; absent exclude axes match every value. */
const ruleSchema = z.strictObject({
  types: choices(ids(catalog.types)).optional(),
  distributions: choices(ids(catalog.distributions)).optional(),
  lengths: choices(
    z.union([
      z.number().int().positive().max(Number.MAX_SAFE_INTEGER),
      z.enum(["insertion-max", "insertion-next", "tile-next"]),
    ]),
  ).optional(),
  operation: operationSchema.optional(),
  executions: choices(execution).optional(),
});
const schema = z.strictObject({
  $comment: z.string().optional(),
  schema: z.literal(3, { error: "unsupported plan schema; regenerate the plan for schema 3" }),
  implementations: choices(ids(catalog.implementations)),
  include: z.array(ruleSchema).min(1),
  exclude: z.array(ruleSchema).default([]),
});

type Rule = z.infer<typeof ruleSchema>;
/** A validated experiment selection, independent of its measurement protocol. */
export type Plan = z.infer<typeof schema>;
export type SelectionOptions = Pick<Rule, "types" | "distributions" | "lengths"> & {
  implementations?: string[];
  /** Keeps the rules without an operation and gives them this one. */
  operation?: Operation["kind"];
  index?: number;
  bucketCount?: number;
  /** Replaces every include rule's executions. */
  executions?: Execution[];
};

/** Validate a selection without choosing a measurement protocol or adding implementations. */
export function parsePlan(value: unknown): Plan {
  const plan = schema.parse(value);
  validateOperations(plan);
  return plan;
}

const sameOperation = (a: Operation, b: Operation) =>
  JSON.stringify(a, Object.keys(a).sort()) === JSON.stringify(b, Object.keys(b).sort());

/** Check operation parameters on every surviving literal count, as Rust does on its
 * expanded cases; engine-relative boundaries resolve only in Rust, for sorting. */
function validateOperations(plan: Plan) {
  const nonSort = plan.include.filter((rule) => rule.operation && rule.operation.kind !== "sort");
  if (nonSort.some((rule) => rule.lengths?.some((n) => typeof n !== "number")))
    throw new Error("Non-sort rules require literal record counts");
  for (const rule of nonSort) {
    const operation = rule.operation!;
    const executions = rule.executions ?? ["serial"];
    for (const len of (rule.lengths ?? catalog.inputs.lengths) as number[]) {
      const survives = (rule.types ?? catalog.types.map((t) => t.id)).some((ty) =>
        (rule.distributions ?? catalog.distributions.map((d) => d.id)).some((distribution) =>
          executions.some(
            (execution) =>
              !plan.exclude.some(
                (r) =>
                  (!r.types || r.types.includes(ty)) &&
                  (!r.distributions || r.distributions.includes(distribution)) &&
                  (!r.lengths || r.lengths.includes(len)) &&
                  (!r.operation || sameOperation(r.operation, operation)) &&
                  (!r.executions || r.executions.includes(execution)),
              ),
          ),
        ),
      );
      if (survives) validateOperation(operation, len);
    }
  }
}

const PRESETS = ["local", "continuous", "smoke", "release", "parallel", "growth"];

/** Load a TOML preset or a JSON/TOML file, then apply explicit axis overrides. */
export async function loadPlan(name: string, options: SelectionOptions = {}): Promise<Plan> {
  const path = PRESETS.includes(name)
    ? fileURLToPath(new URL(`./plans/${name}.toml`, import.meta.url))
    : name;
  const plan = schema.parse(decodeConfig(await readFile(path, "utf8"), path));
  const { implementations, operation, index, bucketCount, executions, ...axes } = options;
  if (operation === undefined && (index !== undefined || bucketCount !== undefined))
    throw new Error("Operation parameters require --operation");
  const requested =
    operation === undefined
      ? undefined
      : operationSchema.parse({
          kind: operation,
          ...(index === undefined ? {} : { index }),
          ...(bucketCount === undefined ? {} : { bucket_count: bucketCount }),
        });
  // A requested operation replaces the plan's operation rules: it applies to the sorting rules,
  // whose inputs every operation shares.
  const include = requested ? plan.include.filter((rule) => !rule.operation) : plan.include;
  if (!include.length) throw new Error("The plan has no inputs outside its operation rules");
  return parsePlan({
    ...plan,
    implementations: implementations ?? plan.implementations,
    include: include.map((rule) => ({
      ...rule,
      ...axes,
      ...(requested && requested.kind !== "sort" ? { operation: requested } : {}),
      ...(executions ? { executions } : {}),
    })),
  });
}

/** Every operation an include rule selects, defaulting to sort. */
export function planOperations(plan: Plan) {
  const operations: Operation[] = [];
  for (const rule of plan.include) {
    const operation = rule.operation ?? SORT;
    if (!operations.some((known) => sameOperation(known, operation))) operations.push(operation);
  }
  return operations;
}

/**
 * The plan split into one plan per operation, in `planOperations` order. A measurement session
 * measures one operation, so a plan that selects several runs one session per part.
 */
export function plansByOperation(plan: Plan): Plan[] {
  return planOperations(plan).map((operation) => ({
    ...plan,
    include: plan.include.filter((rule) => sameOperation(rule.operation ?? SORT, operation)),
  }));
}

/** Every execution an include rule selects, serial first, then ascending worker counts. */
export function planExecutions(plan: Plan): Execution[] {
  return [
    ...new Set(plan.include.flatMap((rule) => rule.executions ?? ["serial" as const])),
  ].toSorted(compareExecutions);
}

/** Export the same validation rules for editor completion and external tooling. */
export function planSchema() {
  return z.toJSONSchema(schema, { io: "input" });
}

/** Apply an optional CLI pattern to IDs resolved by the native harness. */
export function selectCaseIds(ids: string[], pattern?: string) {
  const filter = pattern === undefined ? undefined : new RegExp(pattern);
  const selected = ids.filter((id) => !filter || filter.test(id));
  if (!selected.length) throw new Error("The benchmark plan selects no supported cases");
  return selected;
}
