// SPDX-License-Identifier: BSD-2-Clause

import { appendFile } from "node:fs/promises";
import { type Column, catalog, columnId, type Execution } from "../catalog.ts";
import { TRACKING_SPEC } from "./result.ts";

/** Track implementations and composition controls with independent absolute times. */
export const trackingImplementations = catalog.implementations.filter(
  (entry) => entry.adapter !== "revision",
);

/**
 * Pool sizes tracked for parallel entry points: powers of two up to the tracking host's CPUs,
 * so the history shows scaling without oversubscribing the host.
 */
const trackingWorkers = Array.from(
  { length: Math.floor(Math.log2(TRACKING_SPEC.cpus)) + 1 },
  (_, i) => `t${2 ** i}` as Execution,
);

/** Every tracked column, one remote run each, in catalog and execution order. */
export const trackingColumns: Column[] = trackingImplementations.flatMap((entry) =>
  [
    ...(entry.parallel === "only" ? [] : ["serial" as const]),
    ...(entry.parallel ? trackingWorkers : []),
  ].map((execution) => ({ implementation: entry.id, execution })),
);

/** Resolve a catalog ID before it enters a command, data directory or Cloud branch. */
export function trackingImplementation(id: string) {
  const entry = trackingImplementations.find((entry) => entry.id === id);
  if (!entry) throw new Error(`Unknown tracking implementation: ${id}`);
  return entry;
}

/** Resolve a column ID to a tracked column. */
export function trackingColumn(id: string): Column {
  const column = trackingColumns.find((entry) => columnId(entry) === id);
  if (!column) throw new Error(`Unknown tracking column: ${id}`);
  return column;
}

/**
 * Bencher names benchmarks by Criterion ID, which includes the execution, so the columns of an
 * implementation share its Cloud branch and the implementations need their own.
 */
export function trackingBranch(id: string) {
  return id === "tessera" ? "main" : `implementation-${trackingImplementation(id).id}`;
}

/**
 * The columns a workflow selection measures: `all`, a column ID, or an implementation ID. An
 * implementation measures the columns the continuous plan selects for it, so a main push keeps
 * PR coverage: Tessera serially and at the plan's pool size.
 */
export function trackingSelection(selection: string, planExecutions: Execution[]): Column[] {
  if (selection.includes(","))
    return selection.split(",").flatMap((id) => trackingSelection(id, planExecutions));
  if (selection === "all") return trackingColumns;
  if (selection.includes("/")) return [trackingColumn(selection)];
  const id = trackingImplementation(selection).id;
  const columns = trackingColumns.filter(
    (column) => column.implementation === id && planExecutions.includes(column.execution),
  );
  if (!columns.length) throw new Error(`The continuous plan selects no column of ${id}`);
  return columns;
}

/** A column's file-safe name, for artifacts and images. */
export function columnSlug(column: Column) {
  return `${column.implementation}-${column.execution}`;
}

// Workflow matrix: one entry per selected column.
if (import.meta.main) {
  const { loadPlan, planExecutions } = await import("../plan.ts");
  const columns = trackingSelection(
    process.env.TRACKING_SELECTION ?? "tessera",
    planExecutions(await loadPlan("continuous")),
  );
  const matrix = JSON.stringify({
    include: columns.map((column) => ({
      column: columnId(column),
      slug: columnSlug(column),
      implementation: column.implementation,
      native: trackingImplementation(column.implementation).adapter === "native",
      go: trackingImplementation(column.implementation).adapter === "go",
    })),
  });
  if (process.env.GITHUB_OUTPUT) await appendFile(process.env.GITHUB_OUTPUT, `matrix=${matrix}\n`);
  else console.log(matrix);
}
