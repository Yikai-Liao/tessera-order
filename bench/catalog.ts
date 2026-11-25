// SPDX-License-Identifier: BSD-2-Clause

import { readFileSync } from "node:fs";
import { z } from "zod";
import { decodeConfig } from "./config.ts";
import type { Execution } from "./execution.ts";
import { executionThreads, parseExecution } from "./execution.ts";

export {
  compareExecutions,
  type Execution,
  executionThreads,
  parseExecution,
} from "./execution.ts";

const identity = z.object({ id: z.string(), label: z.string() });
const blockBudget = z.object({
  count: z.number().int().positive(),
  agreement: z.number().int().positive(),
});
const inputDefaults = z.object({ lengths: z.array(z.number().int().positive()) });

/** Language-neutral benchmark identities, presentation metadata and plan order. */
export const catalog = z
  .object({
    schema: z.literal(3),
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
    implementations: z.array(
      identity.extend({
        stable: z.boolean(),
        family: z.string(),
        adapter: z.string(),
        /** A parallel full-sort entry point beside the serial one, or instead of it. */
        parallel: z.enum(["also", "only"]).optional(),
        detail: z.string().optional(),
        description: z.string().optional(),
        /** Why the memory curve has an unexpected shape, as a sentence for the memory figure. */
        memory: z.string().optional(),
        /** The call each listed operation makes, where the label names only the sort. */
        routes: z
          .record(z.string(), z.strictObject({ call: z.string(), stable: z.boolean().optional() }))
          .default({}),
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
    continuous: z.object({ testbed: z.string(), features: z.array(z.string()) }),
    inputs: inputDefaults,
    operations: z.array(
      identity.extend({
        parallel_label: z.string().optional(),
        /** The implementation whose serial column is the speed denominator. */
        reference: z.string(),
        /** The implementation whose column at the same pool size is the parallel denominator. */
        parallel_reference: z.string().optional(),
        description: z.string(),
        /** One call in figure text: "sort", "selection". */
        noun: z.string(),
        parameters: z.array(z.object({ id: z.string(), label: z.string().optional() })).default([]),
        comparison: z
          .object({ types: z.array(z.string()), summaryDistributions: z.array(z.string()) })
          .optional(),
      }),
    ),
  })
  .refine(
    (catalog) =>
      catalog.operations.every(
        (operation) =>
          catalog.implementations.some(
            (entry) => entry.id === operation.reference && entry.parallel !== "only",
          ) &&
          (operation.parallel_reference === undefined ||
            catalog.implementations.some(
              (entry) => entry.id === operation.parallel_reference && entry.parallel,
            )),
      ),
    "An operation reference must name an implementation with that entry point",
  )
  .refine(
    (catalog) =>
      catalog.implementations.every((entry) =>
        Object.keys(entry.routes).every((id) =>
          catalog.operations.some((operation) => operation.id === id && id !== "sort"),
        ),
      ),
    "An implementation route must name a non-sort operation",
  )
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

const implementations = index(catalog.implementations);
const records = index(catalog.types);
const distributions = index(catalog.distributions);
const operations = index(catalog.operations);

/** Look up an implementation's presentation without parsing its identifier. */
export function implementation(id: string): (typeof catalog.implementations)[number] {
  return (
    implementations.get(id) ?? {
      id,
      label: id,
      family: "comparison",
      adapter: "unknown",
      stable: false,
      routes: {},
    }
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

const count = z.number().int().min(0).max(Number.MAX_SAFE_INTEGER);

/** Operations and their exact parameters; the catalog orders kinds and parameters. */
export const operationSchema = z.discriminatedUnion("kind", [
  z.strictObject({ kind: z.literal("sort") }),
  z.strictObject({ kind: z.literal("argsort") }),
  z.strictObject({ kind: z.literal("select_nth"), index: count }),
  z.strictObject({ kind: z.literal("partition-buckets"), bucket_count: count.min(1) }),
]);
export type Operation = z.infer<typeof operationSchema>;
/** Operation kinds in catalog (report section) order. */
export const operationKinds = catalog.operations.map((entry) => entry.id) as Operation["kind"][];
const schemaKinds = operationSchema.options.map((schema) => schema.shape.kind.value);
if (
  operationKinds.length !== schemaKinds.length ||
  !schemaKinds.every((kind) => operationKinds.includes(kind))
)
  throw new Error("The catalog and operation schema name different operations");

/** The full sort, the default operation of plan rules. */
export const SORT: Operation = { kind: "sort" };

/** Validate parameters against a resolved count before building or allocating inputs. */
export function validateOperation(operation: Operation, len: number) {
  if (operation.kind === "select_nth" && operation.index >= len)
    throw new Error(`Operation parameters exceed input length ${len}`);
}

/** An operation's catalog entry and its parameter values in identity order. */
export function operationParts(operation: Operation) {
  const entry = operations.get(operation.kind)!;
  const values = entry.parameters.map(
    (parameter) => (operation as Record<string, unknown>)[parameter.id] as number,
  );
  return { entry, values };
}

/** Each parameter with its optional label, for presentation. */
function parameterText(operation: Operation) {
  const { entry, values } = operationParts(operation);
  return entry.parameters.map((parameter, i) =>
    [parameter.label, values[i].toLocaleString("en-US")].filter(Boolean).join(" "),
  );
}

/** Long report heading, such as `Stable selection · rank 32,768`. */
export function operationDescription(operation: Operation) {
  return [operations.get(operation.kind)!.description, ...parameterText(operation)].join(" · ");
}

/** Short section and cell name, such as `select_nth · rank 32,768`. */
export function operationTitle(operation: Operation) {
  return [operations.get(operation.kind)!.label, ...parameterText(operation)].join(" · ");
}

/** The fixed cross-type aggregate of an operation's comparison report, if it has one. */
export function operationComparison(kind: Operation["kind"]) {
  return operations.get(kind)!.comparison;
}

/** One operation on one seed-one input: the unit that a session pairs. */
export interface CaseIdentity {
  ty: string;
  distribution: string;
  len: number;
  operation: Operation;
}

/** What a session measures on a case: one implementation at one execution. */
export interface Column {
  implementation: string;
  execution: Execution;
}

/** Format `operation/type/distribution/length/s1[/parameters]`. */
export function caseId(input: CaseIdentity) {
  const { entry, values } = operationParts(input.operation);
  return [entry.id, input.ty, input.distribution, input.len, "s1", ...values].join("/");
}

/** Format the resumable group `operation/type/distribution[/parameters]`. */
export function caseGroup(input: CaseIdentity) {
  const { entry, values } = operationParts(input.operation);
  return [entry.id, input.ty, input.distribution, ...values].join("/");
}

const canonicalCount = (text: string | undefined) =>
  text !== undefined && /^(0|[1-9]\d*)$/.test(text) && Number.isSafeInteger(Number(text))
    ? Number(text)
    : undefined;

/** Decode a case ID with catalog identities and valid parameters. */
export function parseCaseId(id: string): CaseIdentity {
  const invalid = () => new Error(`Invalid case ID: ${id}`);
  const [kind, ty, distributionId, length, seed, ...rest] = id.split("/");
  const entry = operations.get(kind);
  const len = canonicalCount(length);
  if (
    !entry ||
    rest.length !== entry.parameters.length ||
    !records.has(ty) ||
    !distributions.has(distributionId) ||
    !len ||
    seed !== "s1"
  )
    throw invalid();
  const candidate: Record<string, unknown> = { kind };
  for (const [i, parameter] of entry.parameters.entries()) {
    const value = canonicalCount(rest[i]);
    if (value === undefined) throw invalid();
    candidate[parameter.id] = value;
  }
  const parsed = operationSchema.safeParse(candidate);
  if (!parsed.success) throw invalid();
  validateOperation(parsed.data, len);
  return { ty, distribution: distributionId, len, operation: parsed.data };
}

/** Format `implementation/execution`. */
export function columnId(column: Column) {
  return `${column.implementation}/${column.execution}`;
}

/** Decode a column with a catalog implementation. */
export function parseColumnId(id: string): Column {
  const separator = id.indexOf("/");
  const implementationId = id.slice(0, separator);
  if (separator < 0 || !implementations.has(implementationId))
    throw new Error(`Invalid column ID: ${id}`);
  return { implementation: implementationId, execution: parseExecution(id.slice(separator + 1)) };
}

/** Format the Criterion ID `rust/<case>/<execution>`. */
export function criterionId(input: CaseIdentity, execution: Execution) {
  return `rust/${caseId(input)}/${execution}`;
}

/** Decode a Criterion ID into its case and execution. */
export function parseCriterionId(id: string): CaseIdentity & { execution: Execution } {
  const separator = id.lastIndexOf("/");
  if (!id.startsWith("rust/") || separator < 0) throw new Error(`Invalid Criterion ID: ${id}`);
  return {
    ...parseCaseId(id.slice("rust/".length, separator)),
    execution: parseExecution(id.slice(separator + 1)),
  };
}

/** Decode a Criterion ID into display fields, retaining an undecodable ID as its input. */
export function caseParts(id: string) {
  let parsed: ReturnType<typeof parseCriterionId>;
  try {
    parsed = parseCriterionId(id);
  } catch {
    return { type: "", input: id, operation: "", n: "", extra: "", execution: "" };
  }
  const record = recordType(parsed.ty),
    distributionEntry = distribution(parsed.distribution);
  const threads = executionThreads(parsed.execution);
  return {
    type: record.compact ?? record.label,
    input: distributionEntry.compact ?? distributionEntry.label,
    // Operation cases share inputs with sorting cases; figures group them by this name.
    operation: parsed.operation.kind === "sort" ? "" : operationTitle(parsed.operation),
    n: parsed.len.toLocaleString("en-US"),
    extra: threads === undefined ? "" : `${threads} threads`,
    execution: parsed.execution,
  };
}

/** Name a case's figure row: input, then record type and length. */
export function caseName(id: string) {
  const parts = caseParts(id);
  return { input: parts.input, sub: [parts.type, parts.n].filter(Boolean).join(" · "), tag: "" };
}

/** Order Criterion IDs by section, length, record type and input; undecodable IDs last. */
export function compareCases(a: string, b: string) {
  const key = (id: string) => {
    try {
      const { ty, distribution: distributionId, len } = parseCriterionId(id);
      const rank = (list: { id: string }[], value: string) =>
        list.findIndex((entry) => entry.id === value);
      return [
        caseSection(id).order,
        len,
        rank(catalog.types, ty),
        rank(catalog.distributions, distributionId),
      ];
    } catch {
      return [Number.MAX_SAFE_INTEGER];
    }
  };
  const [x, y] = [key(a), key(b)];
  return x.reduce((order, value, i) => order || value - (y[i] ?? 0), 0) || a.localeCompare(b);
}

/** Name a figure section by its catalog entry point and, for parallel calls, worker count.
 * Sections follow catalog operation order, serial first, then ascending worker counts. */
export function executionSection(operation: Operation, execution: Execution) {
  const entry = operations.get(operation.kind)!;
  const threads = executionThreads(execution);
  return {
    title:
      threads === undefined
        ? operationTitle(operation)
        : `${entry.parallel_label ?? entry.label} · ${threads} ${threads === 1 ? "thread" : "threads"}`,
    // Worker counts stay below 2^20 for every supported host.
    order: operationKinds.indexOf(operation.kind) * 2 ** 20 + (threads ?? 0),
  };
}

/** The column every other column on a case is divided by: the operation's reference at the
 * same execution, or none where the operation has no parallel reference. */
export function referenceColumn(operation: Operation, execution: Execution): Column | undefined {
  const entry = operations.get(operation.kind)!;
  const implementation = execution === "serial" ? entry.reference : entry.parallel_reference;
  return implementation === undefined ? undefined : { implementation, execution };
}

/** The display label of an operation's reference implementation at an execution. */
export function referenceLabel(operation: Operation, execution: Execution) {
  const column = referenceColumn(operation, execution);
  return column && routeLabel(column.implementation, operation);
}

/** The call an implementation makes for an operation, where its label names only the sort. */
export function route(id: string, operation: Operation): string | undefined {
  return implementation(id).routes[operation.kind]?.call;
}

/** The stability contract for this operation, inheriting the implementation default. */
export function routeStable(id: string, operation: Operation): boolean {
  const entry = implementation(id);
  return entry.routes[operation.kind]?.stable ?? entry.stable;
}

/** Whether an implementation is a Tessera composition baseline, marked by the `reference` adapter. */
export function isComposition(id: string): boolean {
  return implementation(id).adapter === "reference";
}

/** An implementation's display name for an operation: its label, with the route if it has one. */
export function routeLabel(id: string, operation: Operation) {
  return [implementation(id).label, route(id, operation)].filter(Boolean).join(" · ");
}

/** The noun for one call of an operation in figure text, and its plural. */
export function operationNoun(operation: Operation, count = 1) {
  const noun = operations.get(operation.kind)!.noun;
  return count === 1 ? noun : `${noun}s`;
}

/** The figure section of a Criterion ID; undecodable IDs sort last, untitled. */
export function caseSection(id: string) {
  try {
    const { operation, execution } = parseCriterionId(id);
    return executionSection(operation, execution);
  } catch {
    return { title: "", order: Number.MAX_SAFE_INTEGER };
  }
}

/** Format a case's readable label while preserving undecodable identifiers. */
export function caseLabel(id: string) {
  const parts = caseParts(id);
  return (
    [parts.type, parts.input, parts.operation, parts.n].filter(Boolean).join(" · ") +
    (parts.extra ? `, ${parts.extra}` : "")
  );
}
