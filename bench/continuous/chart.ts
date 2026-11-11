// SPDX-License-Identifier: BSD-2-Clause

import { renderChanges } from "../plot/changes.ts";
import type { NameCell } from "../plot/layout.ts";
import {
  type CaseRow,
  type Configuration,
  changedRows,
  geomean,
  isFaster,
  modeName,
  modes,
} from "./matrix.ts";
import { formatChange, isMajor, MAJOR } from "./report.ts";

// Small improvements fold only when another alert remains visible; regressions
// are never hidden, and the full report table still includes every case.
export const FOLD_FASTER_BELOW = 0.05;

/** Translate a continuous case's fields into its compact display label. */
export function nameCell(row: CaseRow, strong: boolean): NameCell {
  return {
    input: row.parts.input,
    strong,
    tag: row.parts.seed && row.parts.seed !== "s1" ? `seed ${row.parts.seed.slice(1)}` : "",
    sub: [row.parts.type, row.parts.n].filter(Boolean).join(" · "),
  };
}

/** Select Bencher alerts and present their authoritative verdicts to the plot. */
export function chartSvg(configurations: Configuration[], base = "main") {
  const changed = changedRows(configurations);
  if (!configurations.some((configuration) => configuration.summary?.rows.length)) return null;
  const small = (row: CaseRow) =>
    isFaster(row) &&
    row.cells.every((cell) => cell?.verdict !== "improvement" || -cell.change! < FOLD_FASTER_BELOW);
  const shown = changed.some((row) => !small(row)) ? changed.filter((row) => !small(row)) : changed;
  const folded = changed.length - shown.length;
  const groups = [...Map.groupBy(shown, (row) => modeName(row.parts.mode).toUpperCase())].map(
    ([title, rows]) => ({
      title,
      rows: rows.map((row) => ({
        name: nameCell(
          row,
          row.cells.some(
            (cell) =>
              !!cell && cell.verdict !== "within" && cell.verdict !== "missing" && isMajor(cell),
          ),
        ),
        cells: row.cells.map((cell) =>
          cell
            ? {
                change: cell.change,
                value: cell.change === null ? "new" : formatChange(cell.change),
                status:
                  cell.verdict === "regression"
                    ? ("slower" as const)
                    : cell.verdict === "improvement"
                      ? ("faster" as const)
                      : cell.verdict,
                major: isMajor(cell),
              }
            : null,
        ),
      })),
    }),
  );
  // Quiet runs still show a compact mode summary; individual rows keep the
  // existing alert selection and folding rules above.
  if (!changed.length)
    groups.push({
      title: "OVERVIEW",
      rows: modes(configurations).map((mode) => ({
        name: { input: modeName(mode), sub: "geometric mean", tag: "", strong: true },
        cells: configurations.map(({ summary }) => {
          const value = summary && geomean(summary, mode);
          return value
            ? {
                change: value.change,
                value: formatChange(value.change),
                status: "within" as const,
                major: false,
              }
            : null;
        }),
      })),
    });
  const counts = (summary: Configuration["summary"]) => {
    if (!summary) return "not measured";
    const parts = [
      ...(summary.regressions ? [`${summary.regressions} slower`] : []),
      ...(summary.improvements ? [`${summary.improvements} faster`] : []),
    ];
    return parts.join(" · ") || (summary.compared ? "no alerts" : "no baseline");
  };
  return renderChanges({
    title: {
      input: `Run time against ${base}`,
      sub: "each build, own baseline",
      tag: "",
      strong: true,
    },
    headline: `Benchmark changes against ${base}: ${configurations.map((configuration) => `${configuration.name} ${counts(configuration.summary)}`).join("; ")}`,
    description: shown
      .map(
        (row) =>
          `${row.label}: ` +
          configurations
            .map(
              (configuration, index) =>
                `${configuration.name} ${formatChange(row.cells[index]?.change ?? null)}${row.cells[index] && row.cells[index]!.verdict !== "within" ? ` ${row.cells[index]!.verdict}` : ""}`,
            )
            .join(", "),
      )
      .join("; "),
    columns: configurations.map((configuration) => {
      const summary = configuration.summary;
      const value = summary?.geomean;
      return {
        name: configuration.name,
        value: value == null ? "—" : formatChange(value - 1),
        detail: summary ? counts(summary) : "measurement failed",
        tone: value == null ? "muted" : value < 1 ? "primary" : value > 1 ? "radix" : "text",
        failed: !summary,
      };
    }),
    groups,
    folded: folded
      ? `${folded} more faster, each by less than ${Math.round(FOLD_FASTER_BELOW * 100)}%`
      : undefined,
    majorLabel: `${MAJOR * 100}% or more`,
  });
}
