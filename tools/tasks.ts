// SPDX-License-Identifier: BSD-2-Clause

import { mkdir } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { withCommandLog } from "./process.ts";

/** Resolve a new result location without creating the operation-owned directory. */
export function resultDirectory(root: string, out?: string) {
  return resolve(out ?? join(root, "bench/results", new Date().toISOString().replaceAll(":", "-")));
}

/** Emit one compact result, retaining subprocess diagnostics outside the agent's context. */
export async function runTask<T>(
  out: string,
  verbose: boolean,
  operation: () => Promise<T>,
  log = `${out}.log`,
) {
  await mkdir(dirname(out), { recursive: true });
  try {
    const result = verbose ? await operation() : await withCommandLog(log, operation);
    console.log(
      JSON.stringify({ out, ...(verbose ? {} : { log }), result: result ?? "completed" }),
    );
    return result;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(verbose ? message : `${message}; log: ${log}`, { cause: error });
  }
}
