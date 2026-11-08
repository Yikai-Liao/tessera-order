// SPDX-License-Identifier: BSD-2-Clause

import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";

/** Read mean nanoseconds per sort from Criterion's completed estimates. */
export async function readEstimates(directory: string): Promise<Record<string, number>> {
  const values: Record<string, number> = {};
  async function visit(path: string) {
    for (const entry of await readdir(path, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      const child = join(path, entry.name);
      if (entry.name === "new") {
        const benchmark = JSON.parse(await readFile(join(child, "benchmark.json"), "utf8"));
        const estimates = JSON.parse(await readFile(join(child, "estimates.json"), "utf8"));
        const id: unknown = benchmark.full_id;
        const mean: unknown = estimates.mean?.point_estimate;
        if (
          typeof id !== "string" ||
          typeof mean !== "number" ||
          !Number.isFinite(mean) ||
          mean <= 0
        )
          throw new Error(`Invalid Criterion result: ${child}`);
        if (Object.hasOwn(values, id)) throw new Error(`Duplicate Criterion case: ${id}`);
        values[id] = mean;
      } else await visit(child);
    }
  }
  await visit(directory);
  return values;
}
