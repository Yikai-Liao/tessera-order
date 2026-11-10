// SPDX-License-Identifier: BSD-2-Clause

import { catalog } from "./catalog.ts";

/** Resolve a supported paired-block budget without inferring it from partial observations. */
export function pairedBudget(count: number) {
  const budget = Object.values(catalog.pairedBlocks).find((budget) => budget.count === count);
  if (!budget) throw new Error(`Unsupported paired-block budget: ${count}`);
  return budget;
}

export type PairedVerdict = "faster" | "slower" | "within noise" | "inconclusive";

/** Return the median of a nonempty sample. */
export function median(values: number[]) {
  if (!values.length) throw new Error("cannot summarize an empty sample");
  const sorted = [...values].sort((a, b) => a - b);
  const mid = sorted.length >> 1;
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}
/** Return the geometric mean of positive finite values. */
export function geomean(values: number[]) {
  if (!values.length || values.some((v) => !Number.isFinite(v) || v <= 0))
    throw new Error("geometric mean requires positive values");
  return Math.exp(values.reduce((a, b) => a + Math.log(b), 0) / values.length);
}

export interface PairedBlock {
  first: number;
  second: number;
  value: number;
}

/** Summarize a complete declared budget relative to its paired A/A means. */
export function summarizePaired(
  blocks: PairedBlock[],
  expected = catalog.pairedBlocks.extended.count,
) {
  const { count, agreement } = pairedBudget(expected);
  if (blocks.length !== count) throw new Error(`Paired summaries require ${count} complete blocks`);
  const ratios = blocks.map(({ first, second, value }) => value / ((first + second) / 2));
  const aa = blocks.flatMap(({ first, second }) => {
    const mean = (first + second) / 2;
    return [first / mean, second / mean];
  });
  const ratio = median(ratios),
    aaLow = Math.min(...aa),
    aaHigh = Math.max(...aa);
  const verdict: PairedVerdict =
    ratio < aaLow && ratios.filter((value) => value < 1).length >= agreement
      ? "faster"
      : ratio > aaHigh && ratios.filter((value) => value > 1).length >= agreement
        ? "slower"
        : count === catalog.pairedBlocks.short.count
          ? "inconclusive"
          : "within noise";
  return {
    ratios,
    ratio,
    low: Math.min(...ratios),
    high: Math.max(...ratios),
    aaLow,
    aaHigh,
    verdict,
  };
}
