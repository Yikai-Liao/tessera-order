// SPDX-License-Identifier: BSD-2-Clause

import { caseName, modeName, modeOrder } from "../catalog.ts";
import { renderTrends, type TrendChart } from "../plot/trends.ts";
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

/** Group continuous history's focus cases and aggregates without leaking its protocol. */
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
  const aggregate = (mode: string) => {
    const inMode = trend.series.filter((entry) => caseParts(entry.name).mode === mode);
    if (!inMode.length) return undefined;
    return trend.runs.map(
      (_, i) =>
        Math.exp(
          inMode.reduce((sum, entry) => sum + Math.log1p(entry.changes[i]), 0) / inMode.length,
        ) - 1,
    );
  };
  const groups: TrendChart["groups"] = apiModes.map((mode) => ({
    title: modeName(mode).toUpperCase(),
    rows: [
      {
        name: { input: "Overall", tag: "", sub: "geometric mean", strong: true },
        aggregate: true,
        cells: [cell(aggregate(mode))],
      },
      ...selected
        .filter((name) => caseParts(name).mode === mode)
        .map((name) => {
          const changes = series.get(name)!.changes;
          return {
            name: { ...caseName(name), strong: Math.abs(changes.at(-1)!) >= MAJOR },
            aggregate: false,
            cells: [cell(changes)],
          };
        }),
    ],
  }));
  const progress = (1 + trend.overall.at(-1)!) / (1 + trend.overall[0]) - 1;
  return renderTrends({
    title: { input: "Run time during this PR", sub: "first to latest run", tag: "", strong: true },
    headline: `PR benchmark history: ${BUILD} ${formatChange(progress)} run time over ${trend.runs.length} runs`,
    description: groups
      .flatMap((group) =>
        group.rows.map((row) => {
          const name = [group.title, row.name.input, row.name.sub, row.name.tag]
            .filter(Boolean)
            .join(" · ");
          const history = row.cells[0]?.changes.map(formatChange).join(", ") ?? "—";
          return `${name}: ${BUILD} ${history}`;
        }),
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
    groups,
    minimumSpan: MIN_SWING,
    reference: `${base} ${short(trend.base)} · runs ${short(trend.runs[0].hash)} → ${short(trend.runs.at(-1)!.hash)}`,
    guide: `value: latest run against its ${base} baseline · one scale per row · —: no comparable history`,
  });
}
