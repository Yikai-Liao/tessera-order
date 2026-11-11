// SPDX-License-Identifier: BSD-2-Clause

import { modeName, modeOrder } from "../catalog.ts";
import type { Headline } from "../plot/matrix.ts";
import { renderTrends } from "../plot/trends.ts";
import { MIN_SWING, type TrendSeries, type TrendSummary } from "./history.ts";
import { caseParts, formatChange, MAJOR, short } from "./report.ts";

export interface ConfigurationTrend {
  name: string;
  trend: TrendSummary | null;
}
const swing = (changes: number[]) => (1 + Math.max(...changes)) / (1 + Math.min(...changes)) - 1;

/** Select continuous history's focus cases and supply formatted, protocol-free rows. */
export function trendSvg(configurations: ConfigurationTrend[], base = "main") {
  const measured = configurations.filter((configuration) => configuration.trend);
  if (!measured.length) return null;
  const byName = configurations.map(
    (configuration) =>
      new Map((configuration.trend?.series ?? []).map((series) => [series.name, series])),
  );
  const selected = [
    ...new Set(
      configurations.flatMap(
        (configuration) => configuration.trend?.focus.map((series) => series.name) ?? [],
      ),
    ),
  ];
  const largest = (name: string) =>
    Math.max(
      ...byName
        .map((entries) => entries.get(name))
        .filter((series): series is TrendSeries => !!series)
        .map((series) => swing(series.changes)),
    );
  selected.sort((a, b) => largest(b) - largest(a) || a.localeCompare(b));
  const cell = (changes: number[] | undefined) =>
    changes
      ? {
          changes,
          value: formatChange(changes.at(-1)!),
          major: Math.abs(changes.at(-1)!) >= MAJOR,
        }
      : null;
  const apiModes = [...new Set(measured.flatMap((c) => c.trend!.modes))].sort(
    (a, b) => modeOrder(a) - modeOrder(b) || a.localeCompare(b),
  );
  const aggregateModes = apiModes.length > 1 ? apiModes : [undefined];
  const aggregate = (trend: TrendSummary | null, mode?: string) => {
    if (!trend) return undefined;
    if (mode === undefined) return trend.overall;
    const series = trend.series.filter((entry) => caseParts(entry.name).mode === mode);
    if (!series.length) return undefined;
    return trend.runs.map(
      (_, i) =>
        Math.exp(
          series.reduce((sum, entry) => sum + Math.log1p(entry.changes[i]), 0) / series.length,
        ) - 1,
    );
  };
  const aggregates = aggregateModes.map((mode) => ({
    name: {
      input: mode === undefined ? "Overall" : modeName(mode),
      tag: "",
      sub: "geometric mean",
      strong: true,
    },
    aggregate: true,
    cells: configurations.map((configuration) => cell(aggregate(configuration.trend, mode))),
  }));
  const rows = [
    ...aggregates,
    ...selected.map((name) => {
      const parts = caseParts(name);
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
          strong: byName.some(
            (entries) => Math.abs(entries.get(name)?.changes.at(-1) ?? 0) >= MAJOR,
          ),
        },
        aggregate: false,
        cells: byName.map((entries) => cell(entries.get(name)?.changes)),
      };
    }),
  ];
  const progress = (configuration: ConfigurationTrend) =>
    configuration.trend
      ? (1 + configuration.trend.overall.at(-1)!) / (1 + configuration.trend.overall[0]) - 1
      : null;
  const columns: Headline[] = configurations.map((configuration) => {
    const value = progress(configuration);
    return {
      name: configuration.name,
      value: formatChange(value),
      detail: configuration.trend ? `${configuration.trend.runs.length} runs` : "no history",
      tone: value == null ? "muted" : value < 0 ? "primary" : value > 0 ? "radix" : "text",
    };
  });
  const bases = [...new Set(measured.map((configuration) => short(configuration.trend!.base)))];
  const ranges = [
    ...new Set(
      measured.map(
        (configuration) =>
          `${short(configuration.trend!.runs[0].hash)} → ${short(configuration.trend!.runs.at(-1)!.hash)}`,
      ),
    ),
  ];
  return renderTrends({
    title: { input: "Run time during this PR", sub: "first to latest run", tag: "", strong: true },
    headline: `PR benchmark history: ${configurations
      .map((configuration) =>
        configuration.trend
          ? `${configuration.name} ${formatChange(progress(configuration))} run time over ${configuration.trend.runs.length} runs`
          : `${configuration.name} unavailable`,
      )
      .join("; ")}`,
    description: [
      ...aggregates.map(
        (row) =>
          `${row.name.input}: ${configurations.map((configuration, index) => `${configuration.name} ${row.cells[index]?.changes.map(formatChange).join(", ") ?? "—"}`).join("; ")}`,
      ),
      ...selected.map(
        (name, index) =>
          `${name}: ${configurations.map((configuration, column) => `${configuration.name} ${rows[index + aggregates.length].cells[column]?.changes.map(formatChange).join(", ") ?? "—"}`).join("; ")}`,
      ),
    ].join(" | "),
    columns,
    rows,
    minimumSpan: MIN_SWING,
    reference: `${base} ${bases.join(", ")} · runs ${ranges.join(", ")}`,
    guide: `value: latest run against its ${base} baseline · one scale per row · —: no comparable history`,
  });
}
