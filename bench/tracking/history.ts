// SPDX-License-Identifier: BSD-2-Clause

import * as Plot from "@observablehq/plot";
import { formatChange, ink, textWidth } from "../plot/annotations.ts";
import { renderFigure } from "../plot/figure.ts";
import { theme } from "../plot/theme.ts";
import { renderTrends, type TrendCell, type TrendChart } from "../plot/trends.ts";

import type { TrackingCase, TrackingRun } from "./result.ts";

// Display choices, not statistical tests. Bare Metal estimates of one build
// usually agree within about 1%, so no chart magnifies a span smaller than
// 5% into its full height; a cumulative change of 20% is set in bold.
const MIN_SPAN = 0.05;
const MAJOR = 0.2;

const seriesOf = (run: TrackingRun) => run.spec;
const short = (hash: string) => hash.slice(0, 7);
const day = (time: string) => time.slice(0, 10);
const commit = (hash: string) => "https://github.com/Yikai-Liao/tessera-sort/commit/" + hash;
const estimate = (run: TrackingRun, id: string) => run.values[id]?.[0];

/** UTC readout shared by inline figures and their host, including time of day. */
export const formatTimestamp = (time: string | number) =>
  new Date(time).toISOString().replace("T", " ").replace("Z", " UTC");
export const historyTimestamp = (run: TrackingRun) => Date.parse(run.commit_time);
export type HistoryAxis = "commit" | "time";
const positions = (runs: TrackingRun[], horizontal: HistoryAxis) =>
  runs.map((run, index) => (horizontal === "time" ? historyTimestamp(run) : index));

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

/** Select the latest measured environment, then order by Git committer time.
 * Ties (including reruns) use measurement time and report identity. Optional
 * inclusive report bounds zoom this same sequence, independently of axis spacing. */
export function comparableRuns(runs: TrackingRun[], range?: { first: string; last: string }) {
  const measured = runs.toSorted(
    (a, b) => Date.parse(a.time) - Date.parse(b.time) || a.id.localeCompare(b.id),
  );
  const latest = measured.at(-1);
  const sorted = latest
    ? measured
        .filter((run) => seriesOf(run) === seriesOf(latest))
        .sort(
          (a, b) =>
            historyTimestamp(a) - historyTimestamp(b) ||
            Date.parse(a.time) - Date.parse(b.time) ||
            a.id.localeCompare(b.id),
        )
    : [];
  if (!range) return sorted;
  const first = sorted.findIndex((run) => run.id === range.first);
  const last = sorted.findIndex((run) => run.id === range.last);
  return first < 0 || last < first ? [] : sorted.slice(first, last + 1);
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
    ).findLast((value) => value !== null) ?? 0;
  return {
    title: {
      input: "Run time on main",
      sub: `${day(first.commit_time)} → ${day(last.commit_time)}`,
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

/** Every case on main, with optional row links and host width; offline callers keep the default size. */
export function renderMainTrend(
  runs: TrackingRun[],
  cases: TrackingCase[],
  id = "main-trend",
  link?: (id: string) => string,
  minimumWidth?: number,
  horizontal: HistoryAxis = "commit",
) {
  const spec = mainTrend(runs, cases, link);
  if (!spec) return "";
  const groups = spec.groups.map((group) => ({ ...group, title: group.title.toUpperCase() }));
  return renderTrends({
    ...spec,
    id,
    groups,
    minimumWidth,
    positions: positions(comparableRuns(runs), horizontal),
    guide: horizontal === "time" ? "Commit time (UTC)" : "Commits · timestamp order",
  });
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

/** One selected case and the theme identity colour that follows it. */
export interface ComparedCase {
  entry: TrackingCase;
  color: (typeof theme.series)[number];
}

/** Plot frame in SVG units. start/end and ComparisonRun.position share units:
 * UTC milliseconds on the time axis, execution slots on the commit axis. */
export interface ComparisonFrame {
  width: number;
  height: number;
  left: number;
  right: number;
  top: number;
  bottom: number;
  start: number;
  end: number;
}

/** Labelled ticks for a logarithmic time axis: 1-2-5 steps across a wide range, even steps across a narrow one. */
function timeTicks(low: number, high: number) {
  const wide: number[] = [];
  for (let power = Math.floor(Math.log10(low)); power <= Math.ceil(Math.log10(high)); power++)
    for (const mantissa of [1, 2, 5]) {
      const value = mantissa * 10 ** power;
      if (value >= low && value <= high) wide.push(value);
    }
  return wide.length >= 3 ? wide : evenTicks(low, high);
}

/** Four to seven ticks at a 1, 2 or 5 step; rounding keeps decimal steps exact for labels. */
function evenTicks(low: number, high: number) {
  const raw = (high - low) / 6,
    magnitude = 10 ** Math.floor(Math.log10(raw));
  const step = magnitude * ([1, 2, 5, 10].find((factor) => factor * magnitude >= raw) ?? 10);
  const ticks: number[] = [];
  for (let index = Math.ceil(low / step); index * step <= high; index++)
    ticks.push(Number((index * step).toPrecision(12)));
  return ticks;
}

/** The comparison's vertical axis: absolute run time, or change from each case's first run. */
export type ComparisonAxis = "time" | "change";

/** One comparable run with each compared case's measurement, in selection order. */
export interface ComparisonRun {
  position: number;
  id: string;
  time: number;
  commitTime: string;
  hash: string;
  values: ({ value: number; lower: number; upper: number; change: number } | null)[];
}

const formatPercent = (value: number) => {
  const pct = Number((value * 100).toFixed(1));
  return `${pct > 0 ? "+" : pct < 0 ? "−" : ""}${Math.abs(pct)}%`;
};

/** Overlay the selected cases on comparable runs, as absolute time or as change from each case's first run.
 *
 * Time shows which case is faster; change shows which moved, the quantity the
 * trend rows show. The returned runs carry both for the host page's readout.
 * A host width expands the time axis, keeping the height and typography unchanged.
 */
export function renderComparison(
  input: TrackingRun[],
  compared: ComparedCase[],
  axis: ComparisonAxis = "time",
  id = "main-compare",
  minimumWidth = 0,
  horizontal: HistoryAxis = "time",
) {
  const runs = comparableRuns(input);
  const baselines = compared.map(({ entry }) => {
    const first = runs.find((run) => run.values[entry.id]);
    return first ? { hash: first.hash, time: first.time, value: first.values[entry.id][0] } : null;
  });
  if (!baselines.some(Boolean)) return null;
  // Keep every slot shared with the trend. Missing measurements break a line
  // instead of silently bridging a commit that was not measured for this case.
  const xs = positions(runs, horizontal);
  const table: ComparisonRun[] = runs.map((run, runIndex) => ({
    position: xs[runIndex],
    id: run.id,
    time: Date.parse(run.time),
    commitTime: run.commit_time,
    hash: run.hash,
    values: compared.map(({ entry }, index) => {
      const measured = run.values[entry.id],
        first = baselines[index];
      if (!measured || !first) return null;
      const [value, lower, upper] = measured;
      return { value, lower, upper, change: value / first.value - 1 };
    }),
  }));
  const { pad } = theme.layout;
  const width = Math.max(theme.layout.width, minimumWidth);
  const height = 252,
    top = 20,
    bottom = height - 32;
  const DAY_MS = 86400000,
    INSET = 10;
  const first = xs[0],
    last = xs.at(-1)!;
  const padding = horizontal === "time" ? DAY_MS : 1;
  const [start, end] = first === last ? [first - padding, last + padding] : [first, last];
  const ys = (point: { value: number; lower: number; upper: number }, base: number) =>
    axis === "time"
      ? { y: point.value, lower: point.lower, upper: point.upper }
      : { y: point.value / base - 1, lower: point.lower / base - 1, upper: point.upper / base - 1 };
  const bounds = table.flatMap(({ values }) =>
    values.flatMap((point, index) => {
      if (!point) return [];
      const { lower, upper } = ys(point, baselines[index]!.value);
      return [lower, upper];
    }),
  );
  // Neither axis magnifies less than MIN_SPAN into its height.
  let y: Plot.ScaleOptions;
  let tick: (value: number) => string;
  let ticks: number[];
  if (axis === "time") {
    const low = Math.min(...bounds),
      high = Math.max(...bounds);
    const ratio = Math.max(high / low, 1 + MIN_SPAN) ** 1.1,
      mid = Math.sqrt(low * high);
    y = { type: "log", domain: [mid / Math.sqrt(ratio), mid * Math.sqrt(ratio)] };
    tick = (value) => formatTime(value);
    ticks = timeTicks(mid / Math.sqrt(ratio), mid * Math.sqrt(ratio));
  } else {
    const low = Math.min(0, ...bounds),
      high = Math.max(0, ...bounds);
    const span = Math.max(high - low, MIN_SPAN) * 1.1,
      mid = (low + high) / 2;
    y = { type: "linear", domain: [mid - span / 2, mid + span / 2] };
    tick = formatPercent;
    ticks = evenTicks(mid - span / 2, mid + span / 2);
  }
  const [yLow, yHigh] = y.domain as [number, number];
  const left = pad + 10 + Math.max(textWidth(tick(yLow), 11), textWidth(tick(yHigh), 11));
  const right = width - pad - 6;
  const marks: Plot.Markish[] = compared.flatMap(({ entry, color }, index) => {
    const points = table.map((run) => {
      const point = run.values[index];
      return {
        x: horizontal === "time" ? new Date(run.position) : run.position,
        ...(point ? ys(point, baselines[index]!.value) : { y: null, lower: null, upper: null }),
        title: point
          ? `${entry.label}\n${short(run.hash)} · measured ${formatTimestamp(run.time)}\nCommit time: ${formatTimestamp(run.commitTime)}\n${formatTime(point.value)} (${formatTime(point.lower)} – ${formatTime(point.upper)})\n${formatChange(point.change)} vs ${short(baselines[index]!.hash)} · measured ${formatTimestamp(baselines[index]!.time)}`
          : "",
        href: commit(run.hash),
      };
    });
    const stroke = `var(--${color})`;
    const className = `series-${color}`;
    return [
      Plot.ruleX(points, {
        x: "x",
        y1: "lower",
        y2: "upper",
        stroke,
        strokeWidth: 3,
        strokeOpacity: 0.3,
        strokeLinecap: "round",
        className,
      }),
      Plot.lineY(points, { x: "x", y: "y", stroke, strokeWidth: 2, className }),
      Plot.dot(
        points.filter((point) => point.y !== null),
        {
          x: "x",
          y: "y",
          r: 3.5,
          fill: stroke,
          stroke: ink.panel,
          strokeWidth: 1.5,
          title: "title",
          href: "href",
          target: "_blank",
          className,
        },
      ),
    ];
  });
  const step = Math.max(1, Math.ceil(xs.length / Math.max(2, Math.floor((right - left) / 100))));
  const axisOptions =
    horizontal === "commit"
      ? {
          ticks: xs.filter((_, index) => index % step === 0 || index === xs.length - 1),
          tickFormat: (index: number) => short(table[index].hash),
        }
      : {
          ticks: end - start <= 2 * DAY_MS ? 5 : end - start <= 7 * DAY_MS ? "day" : 6,
          tickFormat: (date: Date) =>
            end - start <= 2 * DAY_MS
              ? date.toISOString().slice(5, 16).replace("T", " ")
              : date.toLocaleDateString("en-US", {
                  month: "short",
                  day: "numeric",
                  timeZone: "UTC",
                }),
        };
  const svg = renderFigure(
    {
      width,
      height,
      id,
      title: `${axis === "time" ? "Run time" : "Run-time change"} on main: ${compared.map(({ entry }) => entry.label).join(", ")}`,
      description: compared
        .map(({ entry }, index) => {
          const latest = table.findLast((run) => run.values[index])?.values[index];
          return latest
            ? `${entry.label}: ${formatTime(latest.value)}, ${formatChange(latest.change)} vs ${short(baselines[index]!.hash)} (measured ${formatTimestamp(baselines[index]!.time)})`
            : `${entry.label}: no measurements in this range`;
        })
        .join("; "),
    },
    [
      {
        width,
        height,
        marginLeft: left,
        marginRight: width - right,
        marginTop: top,
        marginBottom: height - bottom,
        x:
          horizontal === "time"
            ? { type: "utc", domain: [new Date(start), new Date(end)], inset: INSET }
            : { type: "linear", domain: [start, end], inset: INSET },
        y,
        marks: [
          Plot.gridY({ ticks, stroke: ink.rule, strokeOpacity: 1 }),
          ...(axis === "change"
            ? [Plot.ruleY([0], { stroke: ink.axis, strokeDasharray: "3,3" })]
            : []),
          // Plot overrides custom log-axis formats, so labels are explicit text.
          Plot.text(ticks, {
            y: (value: number) => value,
            text: tick,
            frameAnchor: "left",
            textAnchor: "end",
            dx: -10,
            fill: ink.muted,
            fontSize: 11,
            className: "y-tick",
          }),
          Plot.axisX({
            ...axisOptions,
            tickSize: 4,
            label: null,
            stroke: ink.axis,
            fill: ink.muted,
            fontSize: 11,
          }),
          ...marks,
        ],
      },
    ],
  );
  const frame: ComparisonFrame = {
    width,
    height,
    left: left + INSET,
    right: right - INSET,
    top,
    bottom,
    start,
    end,
  };
  return { svg, frame, runs: table, baselines };
}
