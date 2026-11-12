// SPDX-License-Identifier: BSD-2-Clause

import * as Plot from "@observablehq/plot";
import { formatChange, ink, label, textWidth } from "../plot/annotations.ts";
import { renderFigure } from "../plot/figure.ts";
import { theme } from "../plot/theme.ts";
import { renderTrends, type TrendCell, type TrendChart } from "../plot/trends.ts";

import type { TrackingCase, TrackingRun } from "./result.ts";

// Display choices, not statistical tests. Bare Metal estimates of one build
// usually agree within about 1%, so no chart magnifies a span smaller than
// 5% into its full height; a cumulative change of 20% is set in bold.
const MIN_SPAN = 0.05;
const MAJOR = 0.2;

const seriesOf = (run: TrackingRun) => run.protocol + "/" + run.spec;
const short = (hash: string) => hash.slice(0, 7);
const day = (time: string) => time.slice(0, 10);
const commit = (hash: string) => "https://github.com/Yikai-Liao/tessera-sort/commit/" + hash;
const estimate = (run: TrackingRun, id: string) => run.values[id]?.[0];

const UNITS = [
  [1e9, "s"],
  [1e6, "ms"],
  [1e3, "µs"],
  [1, "ns"],
] as const;
type Unit = (typeof UNITS)[number];
const unitOf = (ns: number): Unit => UNITS.find(([size]) => ns >= size) ?? UNITS[3];
/** Format nanoseconds in the largest unit that keeps the value at least 1. */
export function formatTime(ns: number, unit = unitOf(ns)) {
  return `${Number((ns / unit[0]).toPrecision(4))} ${unit[1]}`;
}

/** Runs comparable with the latest: the same protocol and hardware specification, oldest first. */
export function comparableRuns(runs: TrackingRun[]) {
  const sorted = runs.toSorted((a, b) => a.time.localeCompare(b.time));
  const latest = sorted.at(-1);
  return latest ? sorted.filter((run) => seriesOf(run) === seriesOf(latest)) : [];
}

/** One case's change against its first observation; null where a run did not measure it. */
function caseChanges(runs: TrackingRun[], id: string) {
  const first = runs.map((run) => estimate(run, id)).find((value) => value !== undefined);
  return runs.map((run) => {
    const value = estimate(run, id);
    return value === undefined || first === undefined ? null : value / first - 1;
  });
}

/** Chain-link the geometric mean of each case's ratio to its own previous observation.
 *
 * With a fixed set of cases and no gaps the logarithms telescope, so this
 * equals the geometric mean of each case's change from the first run. A case
 * entering the plan does not move the aggregate by its own level, and a case
 * missing from a run contributes its change across the gap when it returns.
 */
function chainedChanges(runs: TrackingRun[], ids: string[]) {
  let level = 1;
  const previous = new Map<string, number>();
  return runs.map((run) => {
    const measured = ids.filter((id) => estimate(run, id) !== undefined);
    if (!measured.length) return null;
    const logs = measured
      .filter((id) => previous.has(id))
      .map((id) => Math.log(estimate(run, id)! / previous.get(id)!));
    if (logs.length) level *= Math.exp(logs.reduce((sum, value) => sum + value, 0) / logs.length);
    for (const id of measured) previous.set(id, estimate(run, id)!);
    return level - 1;
  });
}

/** Each measured case's latest comparable estimate, interval and change from its first run. */
export function latestValues(input: TrackingRun[], cases: TrackingCase[]) {
  const runs = comparableRuns(input);
  return cases.flatMap((entry) => {
    const last = runs.findLast((run) => run.values[entry.id]);
    if (!last) return [];
    const [value, lower, upper] = last.values[entry.id];
    const change = caseChanges(runs, entry.id).findLast((value) => value !== null) ?? 0;
    // A case absent from the latest run reports its own last measurement, with its time.
    return [{ entry, value, lower, upper, change, time: last.time, current: last === runs.at(-1) }];
  });
}

function cell(changes: (number | null)[]): TrendCell {
  const latest = changes.findLast((value) => value !== null) ?? 0;
  return { changes, value: formatChange(latest), major: Math.abs(latest) >= MAJOR };
}

/** All cases of the comparable runs as one trend matrix, grouped in index order. */
function mainTrend(
  input: TrackingRun[],
  cases: TrackingCase[],
  link?: (id: string) => string,
): TrendChart | null {
  const runs = comparableRuns(input);
  const measured = cases.filter((entry) => runs.some((run) => estimate(run, entry.id)));
  if (!measured.length) return null;
  // The site targets ES2023, so group without Map.groupBy; insertion order keeps index order.
  const byGroup = new Map<string, TrackingCase[]>();
  for (const entry of measured)
    byGroup.set(entry.group, [...(byGroup.get(entry.group) ?? []), entry]);
  const groups = [...byGroup].map(([title, entries]) => ({
    title,
    rows: [
      {
        name: { input: "Overall", sub: "geometric mean", tag: "", strong: true },
        aggregate: true,
        cells: [
          cell(
            chainedChanges(
              runs,
              entries.map((entry) => entry.id),
            ),
          ),
        ],
      },
      ...entries.map((entry) => {
        const changes = cell(caseChanges(runs, entry.id));
        return {
          name: { input: entry.input, sub: entry.sub, tag: entry.tag, strong: changes.major },
          aggregate: false,
          cells: [changes],
          href: link?.(entry.id),
        };
      }),
    ],
  }));
  const first = runs[0],
    last = runs.at(-1)!;
  const overall =
    chainedChanges(
      runs,
      measured.map((entry) => entry.id),
    ).at(-1) ?? 0;
  return {
    title: {
      input: "Run time on main",
      sub: `${day(first.time)} → ${day(last.time)}`,
      tag: "",
      strong: true,
    },
    headline: `Main performance: Tessera ${formatChange(overall)} run time over ${runs.length} runs`,
    description: groups
      .flatMap((group) =>
        group.rows.map((row) => {
          const name = [group.title, row.name.input, row.name.sub, row.name.tag]
            .filter(Boolean)
            .join(" · ");
          return `${name}: ${row.cells[0].value}`;
        }),
      )
      .join(" | "),
    columns: [
      {
        name: "Tessera",
        value: formatChange(overall),
        detail: `${runs.length} ${runs.length === 1 ? "run" : "runs"}`,
        tone: overall < 0 ? "primary" : overall > 0 ? "radix" : "text",
      },
    ],
    groups,
    minimumSpan: MIN_SPAN,
    reference: "first run",
    guide: "",
  };
}

/** Every case on main, with optional links from rows to a host-page detail view. */
export function renderMainTrend(
  runs: TrackingRun[],
  cases: TrackingCase[],
  id = "main-trend",
  link?: (id: string) => string,
) {
  const spec = mainTrend(runs, cases, link);
  if (!spec) return "";
  const groups = spec.groups.map((group) => ({ ...group, title: group.title.toUpperCase() }));
  return renderTrends({ ...spec, id, groups });
}

/** Only each mode's geometric mean, compact enough for the README. */
export function renderMainOverview(runs: TrackingRun[], cases: TrackingCase[]) {
  const spec = mainTrend(runs, cases);
  if (!spec) return "";
  // Each group's first row is its aggregate; the group name becomes its label.
  const rows = spec.groups.map(({ title, rows: [aggregate] }) => ({
    ...aggregate,
    name: { ...aggregate.name, input: title[0].toUpperCase() + title.slice(1) },
  }));
  return renderTrends({ ...spec, id: "main-overview", groups: [{ title: "OVERVIEW", rows }] });
}

/** One case's absolute time through all runs; each protocol and specification is its own line. */
export function renderCaseHistory(runs: TrackingRun[], entry: TrackingCase, id = "main-case") {
  const points = runs
    .filter((run) => run.values[entry.id])
    .toSorted((a, b) => a.time.localeCompare(b.time))
    .map((run) => {
      const [value, lower, upper] = run.values[entry.id];
      return {
        time: new Date(run.time),
        value,
        lower,
        upper,
        series: seriesOf(run),
        title: `${day(run.time)} · ${short(run.hash)}\n${formatTime(value)} (${formatTime(lower)} – ${formatTime(upper)})\n${run.rustc.split("\n")[0]}`,
        href: commit(run.hash),
      };
    });
  if (!points.length) return "";
  const { width, pad, headerRule } = theme.layout;
  const first = points[0],
    last = points.at(-1)!;
  const unit = unitOf(Math.max(...points.map((point) => point.upper)));
  // The vertical span never magnifies less than MIN_SPAN of the latest estimate.
  const low = Math.min(...points.map((point) => point.lower)),
    high = Math.max(...points.map((point) => point.upper));
  const span = Math.max(high - low, last.value * MIN_SPAN) * 1.1,
    mid = (low + high) / 2;
  const DAY_MS = 86400000;
  const [start, end] =
    first.time.getTime() === last.time.getTime()
      ? [new Date(first.time.getTime() - DAY_MS), new Date(last.time.getTime() + DAY_MS)]
      : [first.time, last.time];
  const plotTop = headerRule + 24,
    plotBottom = plotTop + 210,
    height = plotBottom + 64;
  const tick = (value: number) => formatTime(value, unit);
  const marginLeft =
    pad + 10 + Math.max(textWidth(tick(mid + span / 2), 11), textWidth(tick(mid - span / 2), 11));
  // Relative change stays within the latest series, like the trend; other series are context.
  const since = points.find((point) => point.series === last.series)!;
  const change = last.value / since.value - 1;
  const firstDay = day(since.time.toISOString()),
    lastDay = day(last.time.toISOString());
  const latest = formatTime(last.value);
  const nameRight = width - pad - textWidth(latest, theme.font.headline) - 12;
  const footer = height - 22;
  const annotations: Plot.Markish[] = [
    label(entry.input + (entry.tag ? ` (${entry.tag})` : ""), pad, 36, { strong: true }),
    label([entry.sub, entry.group].filter(Boolean).join(" · "), pad, 51, {
      size: 11,
      tone: ink.muted,
    }),
    label(latest, width - pad, 50, { size: theme.font.headline, strong: true, anchor: "end" }),
    label("latest run", nameRight, 36, { strong: true, anchor: "end" }),
    label(
      since !== last ? `${formatChange(change)} since ${firstDay}` : `measured ${lastDay}`,
      nameRight,
      51,
      { size: 11, tone: ink.muted, anchor: "end" },
    ),
    Plot.ruleY([headerRule], { x1: pad, x2: width - pad, stroke: ink.rule }),
    // The legend names the two marks; the page text explains them.
    Plot.dot([{ x: pad + 4, y: footer - 4 }], {
      x: "x",
      y: "y",
      r: 4,
      fill: theme.palette.blue,
      stroke: ink.panel,
      strokeWidth: 1.5,
    }),
    label("estimate", pad + 14, footer, { size: 11, tone: ink.muted }),
    Plot.ruleX([{ x: pad + 84, y1: footer - 10, y2: footer + 2 }], {
      x: "x",
      y1: "y1",
      y2: "y2",
      stroke: theme.palette.blue,
      strokeWidth: 3,
      strokeOpacity: 0.35,
      strokeLinecap: "round",
    }),
    label("interval", pad + 94, footer, { size: 11, tone: ink.muted }),
  ];
  return renderFigure(
    {
      width,
      height,
      id,
      title: `Main performance history: ${entry.label}, ${entry.group}, latest ${latest}`,
      description: points
        .map(
          (point) =>
            `${day(point.time.toISOString())}: ${formatTime(point.value)} (${formatTime(point.lower)} – ${formatTime(point.upper)})`,
        )
        .join("; "),
    },
    [
      { marks: annotations },
      {
        width,
        height,
        marginLeft,
        marginRight: pad + 6,
        marginTop: plotTop,
        marginBottom: height - plotBottom,
        x: { type: "utc", domain: [start, end], inset: 14 },
        y: { type: "linear", domain: [mid - span / 2, mid + span / 2] },
        marks: [
          Plot.gridY({ ticks: 4, stroke: ink.rule, strokeOpacity: 1 }),
          Plot.axisY({
            ticks: 4,
            tickSize: 0,
            tickPadding: 10,
            tickFormat: tick,
            label: null,
            fill: ink.muted,
            fontSize: 11,
          }),
          // One-line dates; short windows tick whole days rather than hours.
          Plot.axisX({
            ticks: end.getTime() - start.getTime() <= 7 * DAY_MS ? "day" : 5,
            tickFormat: (date: Date) =>
              date.toLocaleDateString("en-US", { month: "short", day: "numeric", timeZone: "UTC" }),
            tickSize: 4,
            label: null,
            stroke: ink.axis,
            fill: ink.muted,
            fontSize: 11,
          }),
          Plot.ruleX(points, {
            x: "time",
            y1: "lower",
            y2: "upper",
            stroke: theme.palette.blue,
            strokeWidth: 3,
            strokeOpacity: 0.35,
            strokeLinecap: "round",
          }),
          Plot.lineY(points, {
            x: "time",
            y: "value",
            z: "series",
            stroke: theme.palette.blue,
            strokeWidth: 2,
          }),
          Plot.dot(points, {
            x: "time",
            y: "value",
            r: 4,
            fill: theme.palette.blue,
            stroke: ink.panel,
            strokeWidth: 1.5,
            title: "title",
            href: "href",
            target: "_blank",
          }),
        ],
      },
    ],
  );
}
