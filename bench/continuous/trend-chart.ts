// SPDX-License-Identifier: BSD-2-Clause

import { modeName, modeOrder } from "../catalog.ts";
import { renderTrends } from "../plot/trends.ts";
import { MIN_SWING, type TrendSummary } from "./history.ts";
import { BUILD, caseParts, formatChange, MAJOR, short } from "./report.ts";

const swing = (changes: number[]) => (1 + Math.max(...changes)) / (1 + Math.min(...changes)) - 1;
const cell = (changes: number[] | undefined) =>
  changes
    ? {
        changes,
        value: formatChange(changes.at(-1)!),
        major: Math.abs(changes.at(-1)!) >= MAJOR,
      }
    : null;

/** Select continuous history's focus cases and supply formatted, protocol-free rows. */
export function trendSvg(trend: TrendSummary, base = "main") {
  const series = new Map(trend.series.map((entry) => [entry.name, entry]));
  const selected = trend.focus
    .map((entry) => entry.name)
    .sort(
      (a, b) => swing(series.get(b)!.changes) - swing(series.get(a)!.changes) || a.localeCompare(b),
    );
  const apiModes = [...trend.modes].sort(
    (a, b) => modeOrder(a) - modeOrder(b) || a.localeCompare(b),
  );
  const aggregate = (mode?: string) => {
    if (mode === undefined) return trend.overall;
    const inMode = trend.series.filter((entry) => caseParts(entry.name).mode === mode);
    if (!inMode.length) return undefined;
    return trend.runs.map(
      (_, i) =>
        Math.exp(
          inMode.reduce((sum, entry) => sum + Math.log1p(entry.changes[i]), 0) / inMode.length,
        ) - 1,
    );
  };
  const aggregates = (apiModes.length > 1 ? apiModes : [undefined]).map((mode) => ({
    name: {
      input: mode === undefined ? "Overall" : modeName(mode),
      tag: "",
      sub: "geometric mean",
      strong: true,
    },
    aggregate: true,
    cells: [cell(aggregate(mode))],
  }));
  const rows = [
    ...aggregates,
    ...selected.map((name) => {
      const parts = caseParts(name);
      const changes = series.get(name)!.changes;
      return {
        name: {
          input: parts.input,
          sub: [parts.type, parts.n].filter(Boolean).join(" · "),
          tag: [
            parts.seed && parts.seed !== "s1" ? `seed ${parts.seed.slice(1)}` : "",
            parts.mode !== "api" ? modeName(parts.mode) : "",
          ]
            .filter(Boolean)
            .join(", "),
          strong: Math.abs(changes.at(-1)!) >= MAJOR,
        },
        aggregate: false,
        cells: [cell(changes)],
      };
    }),
  ];
  const progress = (1 + trend.overall.at(-1)!) / (1 + trend.overall[0]) - 1;
  return renderTrends({
    title: { input: "Run time during this PR", sub: "first to latest run", tag: "", strong: true },
    headline: `PR benchmark history: ${BUILD} ${formatChange(progress)} run time over ${trend.runs.length} runs`,
    description: rows
      .map(
        (row, index) =>
          `${index < aggregates.length ? row.name.input : selected[index - aggregates.length]}: ${BUILD} ${row.cells[0]?.changes.map(formatChange).join(", ") ?? "—"}`,
      )
      .join(" | "),
    columns: [
      {
        name: BUILD,
        value: formatChange(progress),
        detail: `${trend.runs.length} runs`,
        tone: progress < 0 ? "primary" : progress > 0 ? "radix" : "text",
      },
    ],
    rows,
    minimumSpan: MIN_SWING,
    reference: `${base} ${short(trend.base)} · runs ${short(trend.runs[0].hash)} → ${short(trend.runs.at(-1)!.hash)}`,
    guide: `value: latest run against ${base} · one scale per row · —: no comparable history`,
  });
}
