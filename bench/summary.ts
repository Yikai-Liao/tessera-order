// SPDX-License-Identifier: BSD-2-Clause

import type { PairedVerdict } from "./statistics.ts";

/** A measured comparison; the enclosing report names its denominator and interval kind. */
export interface ResultRow {
  id: string;
  ratio: number;
  low: number;
  high: number;
  verdict: PairedVerdict;
  aaLow?: number;
  aaHigh?: number;
}

/** Return bounded, actionable evidence without loading a full report into model context. */
export function resultSummary(rows: ResultRow[], priority: "slower" | "faster" = "slower") {
  const changes = rows
    .filter((row) => row.verdict !== "within noise")
    .toSorted(
      (a, b) =>
        Number(b.verdict === priority) - Number(a.verdict === priority) ||
        Math.abs(Math.log(b.ratio)) - Math.abs(Math.log(a.ratio)) ||
        a.id.localeCompare(b.id),
    );
  return {
    measured: rows.length,
    slower: rows.filter((row) => row.verdict === "slower").length,
    faster: rows.filter((row) => row.verdict === "faster").length,
    withinNoise: rows.filter((row) => row.verdict === "within noise").length,
    inconclusive: rows.filter((row) => row.verdict === "inconclusive").length,
    focus: changes.slice(0, 5).map(({ id, ratio, low, high, verdict, aaLow, aaHigh }) => ({
      id,
      ratio,
      range: [low, high],
      verdict,
      ...(aaLow === undefined ? {} : { aaRange: [aaLow, aaHigh] }),
    })),
    omittedChanges: Math.max(0, changes.length - 5),
  };
}
