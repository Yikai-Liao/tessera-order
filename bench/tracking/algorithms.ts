// SPDX-License-Identifier: BSD-2-Clause

import { appendFile } from "node:fs/promises";
import { catalog } from "../catalog.ts";

/** Track production sort adapters, not revision controls or operation-only references. */
export const trackingAlgorithms = catalog.algorithms.filter(
  (entry) => entry.adapter !== "revision" && entry.adapter !== "reference",
);

/** Resolve a catalog ID before it enters a command, data directory or Cloud branch. */
export function trackingAlgorithm(id: string) {
  const entry = trackingAlgorithms.find((entry) => entry.id === id);
  if (!entry) throw new Error(`Unknown tracking algorithm: ${id}`);
  return entry;
}

/** Bencher names benchmarks by case, so each algorithm needs its own Cloud branch. */
export function trackingBranch(id: string) {
  return id === "tessera" ? "main" : `algorithm-${trackingAlgorithm(id).id}`;
}

// Workflow matrix: one algorithm, or every tracking algorithm for `all`.
if (import.meta.main) {
  const selection = process.env.TRACKING_ALGORITHM ?? "tessera";
  const entries = selection === "all" ? trackingAlgorithms : [trackingAlgorithm(selection)];
  const matrix = JSON.stringify({
    include: entries.map((entry) => ({ algorithm: entry.id, native: entry.adapter === "native" })),
  });
  if (process.env.GITHUB_OUTPUT) await appendFile(process.env.GITHUB_OUTPUT, `matrix=${matrix}\n`);
  else console.log(matrix);
}
