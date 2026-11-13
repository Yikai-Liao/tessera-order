// SPDX-License-Identifier: BSD-2-Clause

import { catalog } from "../catalog.ts";

/** Production adapters are the tracking inventory; revision placements are controls, not algorithms. */
export const trackingAlgorithms = catalog.algorithms
  .filter((entry) => entry.adapter !== "revision")
  .map((entry) => ({ ...entry, path: entry.tracking_path ?? entry.id }));
export type TrackingAlgorithm = (typeof trackingAlgorithms)[number];

/** Resolve a public data path before it enters a command, filesystem path or Cloud branch. */
export function trackingAlgorithm(path: string) {
  const entry = trackingAlgorithms.find((entry) => entry.path === path);
  if (!entry) throw new Error("Unknown tracking algorithm: " + path);
  return entry;
}

/** Tessera retains its existing Cloud history; opponents never write into that branch. */
export function trackingBranch(path: string) {
  return path === "tessera" ? "main" : "algorithm-" + trackingAlgorithm(path).path;
}

if (import.meta.main) {
  const selection = process.env.TRACKING_ALGORITHM ?? "tessera";
  const entries = selection === "all" ? trackingAlgorithms : [trackingAlgorithm(selection)];
  const matrix = JSON.stringify({
    include: entries.map((entry) => ({
      algorithm: entry.path,
      native: entry.adapter === "native",
    })),
  });
  if (process.env.GITHUB_OUTPUT) {
    const { appendFile } = await import("node:fs/promises");
    await appendFile(process.env.GITHUB_OUTPUT, "matrix=" + matrix + "\n");
  } else console.log(matrix);
}
