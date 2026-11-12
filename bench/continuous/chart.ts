// SPDX-License-Identifier: BSD-2-Clause

import { caseName, modeName, modeOrder } from "../catalog.ts";
import { type ChangeChart, renderChanges } from "../plot/changes.ts";
import type { NameCell } from "../plot/layout.ts";
import {
  BUILD,
  type ComparedRow,
  formatChange,
  geomean,
  isMajor,
  MAJOR,
  modes,
  type ReportSummary,
} from "./report.ts";

// Small improvements fold only when another alert remains visible; regressions
// are never hidden, and the full report table still includes every case.
const FOLD_FASTER_BELOW = 0.05;

function nameCell(row: ComparedRow, strong: boolean): NameCell {
  return { ...caseName(row.name), strong };
}

/** Select alerted cases and present their authoritative verdicts to the plot. */
export function chartSvg(summary: ReportSummary, base = "main") {
  if (!summary.rows.length) return null;
  // Rows arrive worst slowdown first, then best speedup; a stable sort by mode
  // keeps that order within each mode.
  const changed = summary.rows
    .filter(
      (row): row is ComparedRow => row.verdict === "regression" || row.verdict === "improvement",
    )
    .sort(
      (a, b) =>
        modeOrder(a.parts.mode) - modeOrder(b.parts.mode) ||
        a.parts.mode.localeCompare(b.parts.mode),
    );
  const small = (row: ComparedRow) =>
    row.verdict === "improvement" && -row.change < FOLD_FASTER_BELOW;
  const shown = changed.some((row) => !small(row)) ? changed.filter((row) => !small(row)) : changed;
  const folded = changed.length - shown.length;
  const groups: ChangeChart["groups"] = [
    ...Map.groupBy(shown, (row) => modeName(row.parts.mode).toUpperCase()),
  ].map(([title, rows]) => ({
    title,
    rows: rows.map((row) => ({
      name: nameCell(row, isMajor(row)),
      cells: [
        {
          change: row.change,
          value: formatChange(row.change),
          status: row.verdict === "regression" ? "slower" : "faster",
          major: isMajor(row),
        },
      ],
    })),
  }));
  // Quiet runs still show a compact mode summary; individual rows keep the
  // existing alert selection and folding rules above.
  if (!changed.length)
    groups.push({
      title: "OVERVIEW",
      rows: modes(summary).map((mode) => {
        const value = geomean(summary, mode);
        return {
          name: { input: modeName(mode), sub: "geometric mean", tag: "", strong: true },
          cells: [
            value
              ? {
                  change: value.change,
                  value: formatChange(value.change),
                  status: "within",
                  major: false,
                }
              : null,
          ],
        };
      }),
    });
  const counts =
    [
      ...(summary.regressions ? [`${summary.regressions} slower`] : []),
      ...(summary.improvements ? [`${summary.improvements} faster`] : []),
    ].join(" · ") || (summary.compared ? "no alerts" : "no baseline");
  const total = summary.geomean;
  return renderChanges({
    title: {
      input: `Run time against ${base}`,
      sub: "each build, own baseline",
      tag: "",
      strong: true,
    },
    headline: `Benchmark changes against ${base}: ${BUILD} ${counts}`,
    description: shown
      .map((row) => `${row.label}: ${BUILD} ${formatChange(row.change)} ${row.verdict}`)
      .join("; "),
    columns: [
      {
        name: BUILD,
        value: total == null ? "—" : formatChange(total - 1),
        detail: counts,
        tone: total == null ? "muted" : total < 1 ? "primary" : total > 1 ? "radix" : "text",
      },
    ],
    groups,
    folded: folded
      ? `${folded} more faster, each by less than ${Math.round(FOLD_FASTER_BELOW * 100)}%`
      : undefined,
    majorLabel: `${MAJOR * 100}% or more`,
  });
}
