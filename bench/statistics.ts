// SPDX-License-Identifier: BSD-2-Clause

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

/** Summarize adjacent A/B ratios with a distribution-free median interval.
 *
 * Exact binomial tails choose order statistics with at least 99% coverage under
 * independent samples. Fresh process pairs capture process-state variation;
 * correlated host interference and source-dependent placement remain outside this model.
 * Require the entire interval to exceed the practical 5% regression threshold.
 */
export function summarizeRatios(ratios: number[]) {
  if (ratios.length < 10 || ratios.some((v) => !Number.isFinite(v) || v <= 0))
    throw new Error("Expected at least ten positive paired ratios");
  const sorted = ratios.toSorted((a, b) => a - b);
  const n = sorted.length;
  let probability = 2 ** -n,
    tail = probability,
    k = 0;
  while (k + 1 < n / 2) {
    const next = (probability * (n - k)) / (k + 1);
    if (2 * (tail + next) > 0.01) break;
    probability = next;
    tail += next;
    k++;
  }
  const low = sorted[k],
    high = sorted[n - k - 1];
  const verdict: "faster" | "slower" | "within noise" =
    high < 0.95 ? "faster" : low > 1.05 ? "slower" : "within noise";
  return { ratio: median(sorted), low, high, verdict };
}

export interface PairedBlock {
  first: number;
  second: number;
  value: number;
}

/** Summarize nine observations relative to their paired A/A means. */
export function summarizePaired(blocks: PairedBlock[]) {
  if (blocks.length !== 9) throw new Error("Paired summaries require nine blocks");
  const ratios = blocks.map(({ first, second, value }) => value / ((first + second) / 2));
  const aa = blocks.flatMap(({ first, second }) => {
    const mean = (first + second) / 2;
    return [first / mean, second / mean];
  });
  const ratio = median(ratios),
    aaLow = Math.min(...aa),
    aaHigh = Math.max(...aa);
  const verdict: "faster" | "slower" | "within noise" =
    ratio < aaLow && ratios.filter((value) => value < 1).length >= 8
      ? "faster"
      : ratio > aaHigh && ratios.filter((value) => value > 1).length >= 8
        ? "slower"
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
