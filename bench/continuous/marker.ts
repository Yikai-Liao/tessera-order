// SPDX-License-Identifier: BSD-2-Clause

/** Current single-backend check name. */
export const BENCHMARK_CHECK = benchmarkCheck("rust");

export function benchmarkCheck(language: "rust" | "go") {
  return `Benchmarks (${language})`;
}

export const MARKER = "tessera-bench-report";

/** Identify a benchmark report for one language and exact source revision. */
export function marker(language: string, hash: string) {
  return `<!-- ${MARKER} language=${language} sha=${hash} -->`;
}
