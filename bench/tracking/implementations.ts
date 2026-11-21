// SPDX-License-Identifier: BSD-2-Clause

import { appendFile } from "node:fs/promises";
import { catalog } from "../catalog.ts";
import { TRACKING_SPEC } from "./result.ts";

/** Track production sort adapters that fit the tracking hardware, not revision or operation controls. */
export const trackingImplementations = catalog.implementations.filter(
  (entry) =>
    entry.adapter !== "revision" &&
    entry.adapter !== "reference" &&
    (entry.threads ?? 1) <= TRACKING_SPEC.cpus,
);

/** Resolve a catalog ID before it enters a command, data directory or Cloud branch. */
export function trackingImplementation(id: string) {
  const entry = trackingImplementations.find((entry) => entry.id === id);
  if (!entry) throw new Error(`Unknown tracking implementation: ${id}`);
  return entry;
}

/** Bencher names benchmarks by case, so each implementation needs its own Cloud branch. */
export function trackingBranch(id: string) {
  return id === "tessera" ? "main" : `implementation-${trackingImplementation(id).id}`;
}

// Workflow matrix: one implementation, or every tracking implementation for `all`.
if (import.meta.main) {
  const selection = process.env.TRACKING_IMPLEMENTATION ?? "tessera";
  const entries =
    selection === "all" ? trackingImplementations : [trackingImplementation(selection)];
  const matrix = JSON.stringify({
    include: entries.map((entry) => ({
      implementation: entry.id,
      native: entry.adapter === "native",
    })),
  });
  if (process.env.GITHUB_OUTPUT) await appendFile(process.env.GITHUB_OUTPUT, `matrix=${matrix}\n`);
  else console.log(matrix);
}
