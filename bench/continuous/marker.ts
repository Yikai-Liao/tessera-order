// SPDX-License-Identifier: BSD-2-Clause

import type { Variant } from "../catalog.ts";

export const MARKER = "tessera-bench-report";

/** Identify the exact-head check for one continuous build configuration. */
export function checkName(variant: Pick<Variant, "name">) {
  return `Benchmarks (rust, ${variant.name})`;
}

/** Select the latest attempt of a named check without depending on API page order. */
export function latestCheck<T extends { id: number; name: string }>(
  checks: readonly T[],
  variant: Pick<Variant, "name">,
): T | undefined {
  const name = checkName(variant);
  let latest: T | undefined;
  for (const check of checks)
    if (check.name === name && (!latest || check.id > latest.id)) latest = check;
  return latest;
}

/** Identify a benchmark report for one language and exact source revision. */
export function marker(language: string, hash: string) {
  return `<!-- ${MARKER} language=${language} sha=${hash} -->`;
}
