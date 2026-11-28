// SPDX-License-Identifier: BSD-2-Clause

// Browser-safe: published-data schemas decode executions without reading the catalog.

/** `serial`, or `tN`: the parallel entry point in an N-worker pool; `t1` is not `serial`. */
export type Execution = "serial" | `t${number}`;

/** Decode the canonical spelling of an execution. */
export function parseExecution(text: string): Execution {
  if (
    text === "serial" ||
    (/^t[1-9]\d*$/.test(text) && Number.isSafeInteger(Number(text.slice(1))))
  )
    return text as Execution;
  throw new Error(`Invalid execution ${JSON.stringify(text)}; expected serial or tN with N >= 1`);
}

export function timedExecution(id: string): Execution | undefined {
  const separator = id.lastIndexOf("/");
  if (separator < 0) return undefined;
  try {
    return parseExecution(id.slice(separator + 1));
  } catch {
    return undefined;
  }
}

/** Pool workers of a parallel execution, absent for the serial entry point. */
export function executionThreads(execution: Execution) {
  return execution === "serial" ? undefined : Number(execution.slice(1));
}

/** Serial first, then ascending worker counts. */
export function compareExecutions(a: Execution, b: Execution) {
  return (executionThreads(a) ?? 0) - (executionThreads(b) ?? 0);
}
