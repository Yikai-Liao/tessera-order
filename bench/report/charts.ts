// SPDX-License-Identifier: BSD-2-Clause

import {
  type CaseIdentity,
  catalog,
  columnId,
  distribution,
  type Execution,
  executionThreads,
  implementation,
  type Operation,
  operationNoun,
  recordType,
  referenceColumn,
  referenceLabel,
  route,
  SORT,
} from "../catalog.ts";
import { type CurveChart, renderCurves } from "../plot/curves.ts";
import type { NameCell } from "../plot/layout.ts";
import {
  type RowChart as Chart,
  type Mark,
  type PlotRow as Row,
  renderRows,
  type Tone,
} from "../plot/rows.ts";
import { geomean, median } from "../statistics.ts";
import {
  caseId,
  type Dataset,
  distributions,
  lengths,
  measurementId,
  type Summary,
  TESSERA,
} from "./model.ts";

/** The serial column through which figures name an implementation. */
const serial = (name: string) => columnId({ implementation: name, execution: "serial" });
/** A parallel column's pool size, as a name suffix; empty for serial columns. */
const threadsLabel = (execution: Execution) => {
  const threads = executionThreads(execution);
  return threads ? `${threads} thread${threads === 1 ? "" : "s"}` : "";
};

export interface Figure {
  name: string;
  /** What the figure compares, as a section heading: "u64 keys by input pattern". */
  subject: string;
  /** One sentence on what is compared and how to read it, shown before the figure. */
  summary: string;
  /** The finding, as the chart's own title. */
  title: string;
  /** The chart description, for callers that render it themselves, such as the site. */
  chart: Chart | CurveChart;
  /** Which template draws `chart`; row charts are the default. */
  kind?: "curves";
  svg: string;
  table: string;
}

const rendered = (chart: Chart) => ({ chart, svg: renderRows(chart) });

/**
 * What the comparison figures read, independent of how it was measured.
 *
 * Local reports derive speeds from paired blocks; main tracking derives them
 * from independent runs. Case IDs are comparison IDs (`u64/random/1000/s1`).
 */
export interface Field {
  /** The operation every case performs; its serial reference is the speed denominator. */
  operation: Operation;
  implementations: { name: string; stable: boolean }[];
  cases: CaseIdentity[];
  /** The reference's time over the implementation's time on one case, or null if either is missing. */
  speed(id: string, implementation: string): number | null;
  /** Nanoseconds per element, or null if unmeasured. */
  nsPerKey(id: string, implementation: string): number | null;
  /** Peak extra heap bytes of one call, or null if unmeasured. */
  peak(id: string, implementation: string): number | null;
}

/** The inputs a figure set summarizes and how its guides name them. */
export interface Cohort {
  /**
   * The ranking's element type, length and input patterns. Lengths are tabulated next to
   * the ranking, which admits sorts measured at all of them; `label` follows
   * "speed vs <reference>, " in its guide.
   */
  ranking: { ty: string; n: number; lengths: number[]; distributions: string[]; label: string };
  /** Length of the input-pattern and element-type figures. */
  n: number;
  /** The input-length figure compares `distributionId` at each of `lengths`, ascending. */
  sweep: { distributionId: string; lengths: number[] };
  /** The short-input figure compares every input pattern of `types` at each of `lengths`. */
  short: { lengths: number[]; types: string[] };
  /** Input patterns every u64 input-pattern figure must have. */
  distributions: string[];
  /** Input patterns the element-type figure compares; every type must have each of them. */
  typeDistributions: string[];
  /** How the element-type figure combines them, after "speed relative to <reference>, ". */
  typeDistributionsLabel: string;
}

// Every figure answers one question on one horizontal axis, so it reads
// without a caption: the title asks, a headline number at the top right
// answers, and each row is one named thing. Words appear once — in the title,
// a column head or an axis end — and the cells hold only numbers, so the eye
// lands on the data rather than on repeated labels. Colour says what kind of
// sort a mark is: blue for Tessera, orange for the other radix sorts and grey
// for comparison sorts, SIMD ones included, so the classic radix-sort weak
// spots show as orange dots falling behind.

// Losses on short skewed inputs reach 50× against SIMD sorts. Beyond 32× a
// mark sits on the edge as a triangle and the value keeps the exact figure.
const RATIO_LIMIT = 32;

const displayName = (id: string) => implementation(id).label;
const family = (id: string) =>
  implementation(id).description ?? `${implementation(id).family} sort`;
/**
 * How one operation's figures name things: its reference, its calls and its cases. A row names
 * an implementation by its label and the operation's route, or the label's detail for a sort.
 */
function terms(operation: Operation) {
  const reference = referenceColumn(operation, "serial")!.implementation;
  const detail = (id: string) => route(id, operation) ?? implementation(id).detail;
  const one = operationNoun(operation);
  return {
    reference,
    referenceLabel: referenceLabel(operation, "serial")!,
    one,
    many: operationNoun(operation, 2),
    title: one[0].toUpperCase() + one.slice(1),
    id: (ty: string, distributionId: string, len: number) =>
      caseId({ ty, distribution: distributionId, len, operation }),
    name: (id: string): NameCell => ({
      input: displayName(id),
      sub: detail(id) ?? "",
      tag: "",
      strong: id === "tessera",
    }),
    // Tables name an implementation with its chart subtitle, so a footnote mark such as
    // Block LSD*'s keeps its note.
    table: (id: string) => (detail(id) ? `${displayName(id)} (${detail(id)})` : displayName(id)),
  };
}
type Terms = ReturnType<typeof terms>;
const DISTRIBUTION_GROUPS = [
  ...Map.groupBy(
    catalog.distributions.filter((entry) => entry.group),
    (entry) => entry.group!,
  ),
].map(([title, entries]) => ({
  title,
  distributions: entries.map((entry) => [entry.id, entry.label] as const),
}));
// Keys compare against every implementation; records only against stable ones, because
// stability is observable there. Types follow catalog order.
const typeGroups = (t: Terms) =>
  [
    { against: `any ${t.one}`, keys: true },
    { against: `stable ${t.many}`, keys: false },
  ].map((group) => ({
    ...group,
    title: `${group.keys ? "Keys" : "Records"} · against ${group.against}`,
    types: catalog.types
      .filter((type) => (type.task === "keys") === group.keys)
      .map((type) => [type.id, type.label, type.detail] as const),
  }));

const LARGE = 1000000;
const SHORT = 1000;

// Two significant digits, except that values near 1 keep two decimals so a
// small difference does not print as "1×". Small positive values keep
// significant digits rather than rounding to zero.
const times = (v: number) =>
  `${v < 0.01 ? v.toPrecision(2) : v >= 100 ? v.toFixed(0) : v < 1.095 ? v.toFixed(2) : Number(v.toPrecision(2))}×`;
const formatLength = (n: number) => n.toLocaleString("en-US");
const formatNs = (v: number) => `${Number(v.toPrecision(3))} ns`;
function formatBytes(n: number) {
  for (const [unit, size] of [
    ["GB", 1e9],
    ["MB", 1e6],
    ["kB", 1e3],
  ] as const)
    if (n >= size) return `${Number((n / size).toPrecision(n / size >= 100 ? 3 : 2))} ${unit}`;
  return `${n} B`;
}
const ordinal = (k: number) => {
  const teen = k % 100 >= 11 && k % 100 <= 13;
  return `${k}${teen ? "th" : (["th", "st", "nd", "rd"][k % 10] ?? "th")}`;
};
function markdownTable(headers: string[], rows: string[][]) {
  return `| ${headers.join(" | ")} |\n| ${headers.map(() => "---").join(" | ")} |\n${rows.map((r) => `| ${r.join(" | ")} |`).join("\n")}\n`;
}

const toneOf = (variant: string): Tone =>
  variant === "tessera"
    ? "primary"
    : implementation(variant).family === "radix"
      ? "radix"
      : "comparison";
// A symmetric speed axis in doublings around the reference, wide enough for
// the data. Left of the centre the labels are fractions, so a tick and a
// value cell read the same way.
// The longest centre label that fits between adjacent labelled doublings: "slice::sort" fits.
const CENTRE_LABEL_CHARS = 12;
function speedScale(ratios: number[], reference: string): Chart["scale"] {
  const reach = Math.max(2, ...ratios.map((r) => Math.min(RATIO_LIMIT, Math.max(r, 1 / r)) * 1.08));
  const span = Math.min(RATIO_LIMIT, 2 ** Math.ceil(Math.log2(reach)));
  const steps = Math.log2(span);
  // Every doubling is labelled while there is room; wider axes, and centre labels longer than
  // the gap to the adjacent doubling holds, such as a reference named by its route, label every
  // other one.
  const every = steps <= 3 && reference.length <= CENTRE_LABEL_CHARS ? 1 : 2;
  const ticks = [{ value: 1, label: reference }];
  for (let k = every; k <= steps; k += every) {
    ticks.push({ value: 2 ** k, label: `${2 ** k}×` }, { value: 2 ** -k, label: `1/${2 ** k}×` });
  }
  return { low: 1 / span, high: span, ticks, origin: 1, clamp: true };
}

// Local report figures compare full sorts only.
const REFERENCE = columnId(referenceColumn(SORT, "serial")!);

// Speed of one column relative to the reference in the same blocks: the
// reference's time over the column's, per block, then the median.
function pairedSpeed(summaries: Map<string, Summary>, id: string, column: string) {
  const std = summaries.get(measurementId("default", id, REFERENCE)),
    s = summaries.get(measurementId("default", id, column));
  if (!std || !s) return null;
  return column === TESSERA ? std.ratio : median(std.ratios.map((r, i) => r / s.ratios[i]));
}

/** A local report's field: serial columns' paired speeds and the memory worker's peaks. */
export function reportField(dataset: Dataset, summaries: Map<string, Summary>): Field {
  return {
    operation: SORT,
    implementations: dataset.manifest.columns
      .filter((column) => column.execution === "serial")
      .map((column) => ({ name: column.implementation, stable: column.stable })),
    cases: dataset.manifest.cases,
    speed: (id, name) => pairedSpeed(summaries, id, serial(name)),
    nsPerKey: (id, name) => summaries.get(measurementId("default", id, serial(name)))?.ns ?? null,
    peak: (id, name) =>
      dataset.memory.find(
        (m) => m.build === "default" && m.column === serial(name) && m.case_id === id,
      )?.peak_extra_bytes ?? null,
  };
}

// The geometric mean of those speeds over cases, or null if one is missing or there are none,
// as when the element types share no input pattern.
function speedOver(field: Field, ids: string[], variant: string) {
  const values = ids.map((id) => field.speed(id, variant));
  return values.length && values.every((v): v is number => v !== null) ? geomean(values) : null;
}
const rankingCases = (t: Terms, { ranking }: Cohort, n: number) =>
  ranking.distributions.map((distributionId) => t.id(ranking.ty, distributionId, n));
/** Elements of one type in a guide: "u64 keys" or "{u32, u32} records". */
const elements = (ty: string) => `${recordType(ty).label} ${recordType(ty).task}`;

const kindLegend: [Tone[], string][] = [
  [["primary"], "Tessera"],
  [["radix"], "other radix sorts"],
  [["comparison"], "comparison sorts"],
];
const SPEED_AXIS: [string, string] = ["← slower", "faster →"];

// With `memory`, each implementation also shows its largest peak over the ranked inputs,
// so one figure states both speed and memory.
function renderRanking(field: Field, cohort: Cohort, memory = false): Figure | null {
  const t = terms(field.operation);
  const worstPeak = (variant: string) => {
    const peaks = rankingCases(t, cohort, cohort.ranking.n).map((id) => field.peak(id, variant));
    return peaks.every((v): v is number => v !== null) ? Math.max(...peaks) : null;
  };
  const entries = field.implementations.map((v) => ({
    name: v.name,
    cell: t.name(v.name),
    kind: family(v.name),
    tone: toneOf(v.name),
    stable: v.stable,
    speeds: cohort.ranking.lengths.map((n) => speedOver(field, rankingCases(t, cohort, n), v.name)),
    peak: memory ? worstPeak(v.name) : null,
  }));
  const at = cohort.ranking.lengths.indexOf(cohort.ranking.n);
  const complete = entries
    .filter((e) => e.speeds.every((v) => v !== null))
    .map((e) => ({ ...e, speeds: e.speeds as number[] }))
    .sort((a, b) => b.speeds[at] - a.speeds[at]);
  const anchor = complete.find((e) => e.cell.input === "Tessera");
  if (!anchor) return null;
  // Records rank Tessera among stable implementations only, because stability is observable
  // there; unstable ones stay in their own group for context.
  const records = recordType(cohort.ranking.ty).task === "records";
  const others = complete.filter((e) => !e.cell.strong && (!records || e.stable));
  const rank = 1 + others.filter((e) => e.speeds[at] > anchor.speeds[at]).length;
  const row = (e: (typeof complete)[number]): Row => ({
    name: e.cell,
    marks: [{ value: e.speeds[at], tone: e.tone }],
    cells: [times(e.speeds[at]), ...(memory ? [e.peak === null ? "–" : formatBytes(e.peak)] : [])],
    strong: e.cell.strong,
  });
  const contenders = `${others.length + 1} ${records ? "stable " : ""}${t.many}`;
  const title =
    rank === 1 ? `Fastest of ${contenders}` : `Ranked ${ordinal(rank)} of ${contenders}`;
  return {
    name: memory ? "overview" : "ranking",
    subject: "Overall ranking",
    summary: `Every ${t.one} on the same ${elements(cohort.ranking.ty)}, ranked by speed relative to ${t.referenceLabel}${memory ? ", with the largest extra memory it allocated" : ""}. ${records ? `Records rank Tessera among stable ${t.many}; unstable ${t.many}, which may reorder equal keys, are shown separately.` : `Stable and unstable ${t.many} are grouped separately.`}`,
    title,
    ...rendered({
      title,
      guide: `${formatLength(cohort.ranking.n)} ${elements(cohort.ranking.ty)} · speed vs ${t.referenceLabel}, ${cohort.ranking.label}`,
      columns: ["Speed", ...(memory ? ["Extra memory"] : [])],
      groups: [
        { title: `Stable ${t.many}`, rows: complete.filter((e) => e.stable).map(row) },
        { title: `Unstable ${t.many}`, rows: complete.filter((e) => !e.stable).map(row) },
      ].filter((g) => g.rows.length),
      scale: speedScale(
        complete.map((e) => e.speeds[at]),
        t.referenceLabel,
      ),
      axis: SPEED_AXIS,
      legend: kindLegend,
    }),
    table: markdownTable(
      [
        t.title,
        "Kind",
        "Stability",
        ...cohort.ranking.lengths.map(
          (n) => `${formatLength(n)} elements, speed vs ${t.referenceLabel}`,
        ),
        ...(memory ? ["Largest extra memory"] : []),
      ],
      complete.map((e) => [
        t.table(e.name),
        e.kind,
        e.stable ? "stable" : "unstable",
        ...e.speeds.map(times),
        ...(memory ? [e.peak === null ? "unmeasured" : formatBytes(e.peak)] : []),
      ]),
    ),
  };
}

interface Standing {
  speeds: { variant: string; speed: number }[];
  tessera: number;
  rank: number;
}
// Where Tessera stands among every eligible implementation on one row, all measured
// against the reference, which therefore sits at the origin and is not drawn.
function standing(field: Field, ids: string[], stableOnly: boolean): Standing | null {
  const variants = field.implementations;
  const speedOf = (variant: string) => speedOver(field, ids, variant);
  const tessera = speedOf("tessera");
  if (tessera === null) return null;
  const speeds = variants
    .filter((v) => v.name !== "tessera" && (!stableOnly || v.stable))
    .map((v) => ({ variant: v.name, speed: speedOf(v.name) }))
    .filter((v): v is { variant: string; speed: number } => v.speed !== null);
  return {
    speeds,
    tessera,
    rank: 1 + speeds.filter((v) => v.speed > tessera).length,
  };
}

function fieldRow(t: Terms, name: NameCell, s: Standing): Row {
  return {
    name,
    swarm: true,
    marks: [
      ...s.speeds
        .filter((v) => v.variant !== t.reference)
        .map((v): Mark => ({ value: v.speed, tone: toneOf(v.variant) })),
      { value: s.tessera, tone: "primary" },
    ],
    cells: [times(s.tessera), ordinal(s.rank)],
    strong: true,
    lead: s.rank === 1,
  };
}

// The finding of a field figure, stated as its title.
function fieldTitle(standings: Standing[], unit: string) {
  const total = standings.length,
    first = standings.filter((s) => s.rank === 1).length,
    top3 = standings.filter((s) => s.rank <= 3).length;
  // "All" reads naturally only past two; two cases read as "both".
  const all = (count: number) => (count === 2 ? `both ${unit}` : `all ${count} ${unit}`);
  // One row states its rank on the only measured one: "Fastest on the only input".
  const only = `the only ${unit.slice(0, -1)}`;
  if (total === 1)
    return first ? `Fastest on ${only}` : `Ranked ${ordinal(standings[0].rank)} on ${only}`;
  if (first === total) return `Fastest on ${all(total)}`;
  // A podium needs more than three implementations in the field.
  const field = Math.max(...standings.map((s) => s.speeds.length + 1));
  const podium = top3 > first && field > 3;
  // Without a first place, the podium is the finding.
  if (!first && podium)
    return top3 === total ? `Top 3 on ${all(total)}` : `Top 3 on ${top3} of ${total} ${unit}`;
  return `Fastest on ${first} of ${total} ${unit}${podium ? `, top 3 on ${top3}` : ""}`;
}

function fieldTable(t: Terms, rows: [string, Standing][], nsPerKey?: number[]) {
  return markdownTable(
    [
      "Row",
      ...(nsPerKey ? ["Tessera time per key"] : []),
      "Tessera rank",
      `Tessera speed vs ${t.referenceLabel}`,
      `Other ${t.many}, fastest first`,
    ],
    rows.map(([label, s], i) => [
      label,
      ...(nsPerKey ? [formatNs(nsPerKey[i])] : []),
      `${ordinal(s.rank)} of ${s.speeds.length + 1}`,
      times(s.tessera),
      [...s.speeds]
        .sort((a, b) => b.speed - a.speed)
        .map((v) => `${t.table(v.variant)} ${times(v.speed)}`)
        .join(", "),
    ]),
  );
}

// Each measured input pattern of one type. Records admit only stable implementations;
// for keys, stability is unobservable and every implementation competes.
function renderInputs(field: Field, cohort: Cohort, name: string, ty = "u64"): Figure | null {
  const t = terms(field.operation);
  const { n } = cohort;
  const patterns = field.cases
    .filter((input) => input.ty === ty && input.len === n)
    .map((input) => input.distribution);
  const stableOnly = recordType(ty).task === "records";
  const plotted: [string, Standing][] = [];
  const nsPerKey: number[] = [];
  const groups = DISTRIBUTION_GROUPS.map((g) => ({
    title: g.title,
    rows: g.distributions
      .filter(([distributionId]) => patterns.includes(distributionId))
      .flatMap(([distributionId, label]) => {
        const id = t.id(ty, distributionId, n);
        const s = standing(field, [id], stableOnly);
        const ns = field.nsPerKey(id, "tessera");
        if (!s || ns === null) return [];
        plotted.push([label, s]);
        nsPerKey.push(ns);
        return [fieldRow(t, { input: label, sub: "", tag: "" }, s)];
      }),
  })).filter((g) => g.rows.length);
  // The element-type figure already shows a type's shared pattern; a panel needs a few more.
  if (plotted.length < (ty === "u64" ? 1 : 3)) return null;
  const standings = plotted.map(([, s]) => s);
  const title = fieldTitle(standings, "inputs");
  return {
    name,
    subject: `${elements(ty)} by input pattern`,
    summary: `Each row is one input pattern. Each dot is one ${stableOnly ? `stable ${t.one} (records keep equal keys in order)` : t.one}, placed by its speed relative to ${t.referenceLabel}; the columns give Tessera's speed and rank.`,
    title,
    ...rendered({
      title,
      guide: `${formatLength(n)} ${elements(ty)} · each dot is one ${stableOnly ? "stable " : ""}${t.one}, placed by its speed relative to ${t.referenceLabel}`,
      columns: ["Speed", "Rank"],
      groups,
      scale: speedScale(
        standings.flatMap((s) => [s.tessera, ...s.speeds.map((v) => v.speed)]),
        t.referenceLabel,
      ),
      axis: SPEED_AXIS,
      legend: kindLegend,
    }),
    table: fieldTable(t, plotted, nsPerKey),
  };
}

// One pattern at each sweep length, one group per type, so a reader follows how Tessera's
// standing changes with length. A type needs two lengths to show a change.
function renderLengths(field: Field, cohort: Cohort): Figure | null {
  const t = terms(field.operation);
  const { distributionId, lengths } = cohort.sweep;
  const plotted: [string, Standing][] = [];
  const firsts: [number, boolean][] = [];
  const groups = typeGroups(t)
    .flatMap((g) =>
      g.types.map(([ty]) => {
        const rows = lengths.flatMap((len) => {
          const s = standing(field, [t.id(ty, distributionId, len)], !g.keys);
          return s ? [[len, s] as const] : [];
        });
        return {
          title: `${elements(ty)} · against ${g.against}`,
          rows:
            rows.length < 2
              ? []
              : rows.map(([len, s]) => {
                  plotted.push([`${elements(ty)} · ${formatLength(len)}`, s]);
                  firsts.push([len, s.rank === 1]);
                  return fieldRow(
                    t,
                    { input: `${formatLength(len)} elements`, sub: "", tag: "" },
                    s,
                  );
                }),
        };
      }),
    )
    .filter((g) => g.rows.length);
  if (!plotted.length) return null;
  const standings = plotted.map(([, s]) => s);
  // The shortest length from which Tessera leads at every longer measured length, if any.
  const from = lengths.find((len) => firsts.every(([at, first]) => at < len || first));
  const title =
    from === lengths[0] || from === undefined
      ? fieldTitle(standings, "cases")
      : `Fastest from ${formatLength(from)} elements up`;
  const input = distribution(distributionId).label;
  return {
    name: "lengths",
    subject: "Speed by input length",
    summary: `Each row is one input length, on the ${input} input. Each dot is one ${t.one}, placed by its speed relative to ${t.referenceLabel}; records compare only stable ${t.many}.`,
    title,
    ...rendered({
      title,
      guide: `${input} input · each dot is one ${t.one}, placed by its speed relative to ${t.referenceLabel}`,
      columns: ["Speed", "Rank"],
      groups,
      scale: speedScale(
        standings.flatMap((s) => [s.tessera, ...s.speeds.map((v) => v.speed)]),
        t.referenceLabel,
      ),
      axis: SPEED_AXIS,
      legend: kindLegend,
    }),
    table: fieldTable(t, plotted),
  };
}

// Every pattern of the short-input types, grouped by length and type, where fixed per-call
// costs and fallbacks decide the result.
function renderShort(field: Field, cohort: Cohort): Figure | null {
  const t = terms(field.operation);
  const { lengths, types } = cohort.short;
  const plotted: [string, Standing][] = [];
  const nsPerKey: number[] = [];
  const distributions = DISTRIBUTION_GROUPS.flatMap((g) => g.distributions);
  const groups = lengths
    .flatMap((len) =>
      types.map((ty) => {
        const stableOnly = recordType(ty).task === "records";
        return {
          title: `${formatLength(len)} ${elements(ty)}`,
          rows: distributions.flatMap(([distributionId, label]) => {
            const id = t.id(ty, distributionId, len);
            const s = standing(field, [id], stableOnly);
            const ns = field.nsPerKey(id, "tessera");
            if (!s || ns === null) return [];
            plotted.push([`${label} · ${formatLength(len)} ${elements(ty)}`, s]);
            nsPerKey.push(ns);
            return [fieldRow(t, { input: label, sub: "", tag: "" }, s)];
          }),
        };
      }),
    )
    .filter((g) => g.rows.length);
  if (!plotted.length) return null;
  const standings = plotted.map(([, s]) => s);
  const title = fieldTitle(standings, "inputs");
  return {
    name: "short",
    subject: "Short inputs",
    summary: `Each row is one input pattern, grouped by length and element type. Each dot is one ${t.one}, placed by its speed relative to ${t.referenceLabel}; records compare only stable ${t.many}.`,
    title,
    ...rendered({
      title,
      guide: `${lengths.map(formatLength).join(" and ")} elements · each dot is one ${t.one}, placed by its speed relative to ${t.referenceLabel}`,
      columns: ["Speed", "Rank"],
      groups,
      scale: speedScale(
        standings.flatMap((s) => [s.tessera, ...s.speeds.map((v) => v.speed)]),
        t.referenceLabel,
      ),
      axis: SPEED_AXIS,
      legend: kindLegend,
    }),
    table: fieldTable(t, plotted, nsPerKey),
  };
}

// Each element type over the cohort's type patterns. Records admit
// only stable implementations, because stability is observable there.
function renderTypes(field: Field, cohort: Cohort): Figure | null {
  const t = terms(field.operation);
  const plotted: [string, Standing][] = [];
  const groups = typeGroups(t)
    .map((g) => ({
      title: g.title,
      rows: g.types.flatMap(([ty, label, sub]) => {
        const ids = cohort.typeDistributions.map((distributionId) =>
          t.id(ty, distributionId, cohort.n),
        );
        const s = standing(field, ids, !g.keys);
        if (!s) return [];
        plotted.push([label, s]);
        return [fieldRow(t, { input: label, sub, tag: "" }, s)];
      }),
    }))
    .filter((g) => g.rows.length);
  // A comparison of element types needs at least two of them.
  if (plotted.length < 2) return null;
  const standings = plotted.map(([, s]) => s);
  const title = fieldTitle(standings, "types");
  return {
    name: "types",
    subject: "Element types",
    summary: `Each row is one key or record type. Each dot is one ${t.one}, placed by its speed relative to ${t.referenceLabel}; records compare only stable ${t.many}.`,
    title,
    ...rendered({
      title,
      guide: `${formatLength(cohort.n)} elements · speed relative to ${t.referenceLabel}, ${cohort.typeDistributionsLabel}`,
      columns: ["Speed", "Rank"],
      groups,
      scale: speedScale(
        standings.flatMap((s) => [s.tessera, ...s.speeds.map((v) => v.speed)]),
        t.referenceLabel,
      ),
      axis: SPEED_AXIS,
      legend: kindLegend,
    }),
    table: fieldTable(t, plotted),
  };
}

/** Each implementation's peak extra memory for one operation on one input pattern at several lengths. */
export interface Growth {
  operation: Operation;
  ty: string;
  distribution: string;
  /** Increasing record counts. */
  lengths: number[];
  /** One peak per length, null where unmeasured. */
  peaks: { name: string; values: (number | null)[] }[];
  /** Provenance appended to the guide, such as "Tessera at 1a2b3c4". */
  source?: string;
}

/** A field's peaks on one type's random input, at every length Tessera's peak was measured. */
export function fieldGrowth(field: Field, ty: string): Growth {
  const id = (len: number) => terms(field.operation).id(ty, "random", len);
  const lengths = [
    ...new Set(
      field.cases
        .filter((input) => input.ty === ty && input.distribution === "random")
        .map((input) => input.len),
    ),
  ]
    .filter((len) => field.peak(id(len), "tessera") !== null)
    .toSorted((a, b) => a - b);
  return {
    operation: field.operation,
    ty,
    distribution: "random",
    lengths,
    peaks: field.implementations.map((v) => ({
      name: v.name,
      values: lengths.map((len) => field.peak(id(len), v.name)),
    })),
  };
}

// The memory axis starts at a kilobyte: smaller peaks are constant bookkeeping, and an
// in-place sort's zero has no place on a logarithmic axis.
const MEMORY_FLOOR = 1e3;

/** Count records compactly: binary prefixes for powers of two, which the sweep uses. */
const countLabel = (n: number) =>
  n >= 2 ** 20 && n % 2 ** 20 === 0
    ? `${n / 2 ** 20}M`
    : n >= 2 ** 10 && n % 2 ** 10 === 0
      ? `${n / 2 ** 10}K`
      : formatLength(n);

/** Growth order of a power law through two points, as the exponent of n. */
const exponent = ([n1, y1]: [number, number], [n2, y2]: [number, number]) =>
  Math.log(y2 / y1) / Math.log(n2 / n1);

export function renderGrowth(growth: Growth): Figure | null {
  const t = terms(growth.operation);
  const { ty, lengths } = growth;
  const tessera = growth.peaks.find((entry) => entry.name === "tessera");
  const measured = (values: (number | null)[]) =>
    lengths.flatMap((n, index) =>
      values[index] === null ? [] : [[n, values[index]] as [number, number]],
    );
  const own = tessera ? measured(tessera.values) : [];
  if (own.length < 2) return null;
  const sorts = growth.peaks
    .map((entry) => ({ ...entry, points: measured(entry.values) }))
    .filter((entry) => entry.points.length);
  // Implementations that never reach the axis are named in a note; the table keeps their values.
  const plotted = sorts.filter((entry) => entry.points.some(([, y]) => y >= MEMORY_FLOOR));
  const small = sorts.filter((entry) => !plotted.includes(entry));
  const last = own.at(-1)!;
  const order = exponent(own.at(-2)!, last);
  const growthName = Math.abs(order - 0.5) <= 0.1 ? "√n" : `n^${order.toFixed(2)}`;
  // Half a decade of headroom above the largest value keeps the top line inside the plot.
  const high = Math.max(...plotted.flatMap((e) => e.points.map(([, y]) => y))) * Math.sqrt(10);
  const title = `Grows as ${growthName}, to ${formatBytes(last[1])} at ${countLabel(last[0])} records`;
  const decades = Array.from(
    { length: Math.floor(Math.log10(high / MEMORY_FLOOR)) + 1 },
    (_, index) => MEMORY_FLOOR * 10 ** index,
  );
  const footnotes = [
    ...new Set(plotted.map((e) => implementation(e.name).detail).filter((d): d is string => !!d)),
  ];
  const note = [
    ...(small.length
      ? [
          `Below ${formatBytes(MEMORY_FLOOR)} at every length: ${small
            .map(
              (e) =>
                `${displayName(e.name)} (${formatBytes(Math.max(...e.points.map(([, y]) => y)))})`,
            )
            .join(", ")}.`,
        ]
      : []),
    ...footnotes,
  ].join(" ");
  const table = markdownTable(
    [t.title, ...lengths.map(formatLength)],
    sorts
      .toSorted((a, b) => (b.values.at(-1) ?? 0) - (a.values.at(-1) ?? 0))
      .map((e) => [t.table(e.name), ...e.values.map((v) => (v === null ? "–" : formatBytes(v)))]),
  );
  const chart: CurveChart = {
    title,
    guide: `${elements(ty)}, ${distribution(growth.distribution).label} input · peak heap allocation during one ${t.one}, beyond the input${growth.source ? ` · ${growth.source}` : ""}`,
    x: {
      low: lengths[0],
      high: lengths.at(-1)!,
      ticks: lengths.map((n) => ({ value: n, label: countLabel(n) })),
      label: "records →",
    },
    y: {
      low: MEMORY_FLOOR,
      high,
      ticks: decades.map((value) => ({ value, label: formatBytes(value) })),
      label: "extra memory",
    },
    curves: plotted.map((e) => ({
      name: displayName(e.name),
      tone: toneOf(e.name),
      points: e.points,
    })),
    references: [
      {
        dotted: true,
        points: lengths.map((n) => [n, last[1] * Math.sqrt(n / last[0])] as [number, number]),
      },
    ],
    legend: [...kindLegend, [["dotted"], "√n slope through Tessera"]],
    ...(note ? { note } : {}),
  };
  return {
    name: "memory",
    subject: "Extra memory by input length",
    summary: `How much memory each ${t.one} allocates beyond its input as the input grows. Both axes are logarithmic, so memory proportional to a power of n is a straight line, and a steeper line grows faster: ${t.many} that copy their input rise with the input itself, and the dotted line has the slope of √n.`,
    title,
    kind: "curves",
    chart,
    svg: renderCurves(chart),
    table,
  };
}

/**
 * Tessera's parallel entry point and Rayon's stable sort at each thread count, as speed
 * relative to Rayon's stable sort on one thread, on each input every thread count measured.
 */
export interface Scaling {
  n: number;
  threads: number[];
  /** Short commits of the Tessera runs, which may predate its latest serial run. */
  tessera: string[];
  inputs: { label: string; tessera: number[]; rayon: number[] }[];
}

export function renderParallel({ n, threads, tessera, inputs }: Scaling): Figure | null {
  if (!inputs.length || !threads.length) return null;
  const named = (t: number) => `${t} thread${t === 1 ? "" : "s"}`;
  const mean = (pick: (input: Scaling["inputs"][number]) => number[], at: number) =>
    geomean(inputs.map((input) => pick(input)[at]));
  const rows = threads.map((t, at) => ({
    threads: t,
    tessera: mean((input) => input.tessera, at),
    rayon: mean((input) => input.rayon, at),
  }));
  const widest = rows.at(-1)!;
  const title = `${times(widest.tessera / widest.rayon)} Rayon's speed on ${named(widest.threads)}`;
  const scale = speedScale(
    rows.flatMap((row) => [row.tessera, row.rayon]),
    "",
  );
  scale.ticks = scale.ticks.map((tick) =>
    tick.value === 1 ? { ...tick, label: "Rayon · 1" } : tick,
  );
  const cells = (values: number[]) => values.map(times);
  return {
    name: "parallel",
    subject: "Parallel sorting",
    summary:
      "Tessera's Rayon entry point against Rayon's own stable par_sort at each thread count, both as speed relative to Rayon on one thread.",
    title,
    ...rendered({
      title,
      guide: `${formatLength(n)} elements · geometric mean over the ${inputs.length} inputs every thread count measured · Tessera at ${tessera.join(", ")}`,
      columns: ["Tessera", "Rayon"],
      groups: [
        {
          title: "Threads",
          rows: rows.map(
            (row): Row => ({
              name: { input: named(row.threads), sub: "", tag: "" },
              marks: [
                { value: row.rayon, tone: "comparison" },
                { value: row.tessera, tone: "primary" },
              ],
              cells: [times(row.tessera), times(row.rayon)],
            }),
          ),
        },
      ],
      scale,
      axis: SPEED_AXIS,
      legend: [
        [["primary"], "Tessera"],
        [["comparison"], "Rayon stable sort"],
      ],
    }),
    table: markdownTable(
      [
        "Input",
        ...threads.map((t) => `Tessera, ${named(t)}`),
        ...threads.map((t) => `Rayon, ${named(t)}`),
      ],
      [
        ...inputs.map((input) => [input.label, ...cells(input.tessera), ...cells(input.rayon)]),
        [
          "Geometric mean",
          ...cells(rows.map((row) => row.tessera)),
          ...cells(rows.map((row) => row.rayon)),
        ],
      ],
    ),
  };
}

/** Plot selected inputs when a plan does not contain a complete release panel. */
function renderSelected(dataset: Dataset, summaries: Map<string, Summary>): Figure[] {
  return dataset.manifest.cases.flatMap((input) => {
    const id = caseId(input);
    const entries = dataset.manifest.columns.flatMap((column) => {
      const summary = summaries.get(measurementId("default", id, column.id));
      return summary ? [{ summary, ...column }] : [];
    });
    if (!entries.length) return [];
    const groups = [
      {
        title: `Implementations · ${dataset.manifest.blocks} blocks`,
        rows: entries.map(
          ({ summary, implementation: name, execution }): Row => ({
            name: {
              ...terms(SORT).name(name),
              sub: [implementation(name).detail, threadsLabel(execution)]
                .filter(Boolean)
                .join(" · "),
            },
            marks: [{ value: summary.ratio, tone: toneOf(name) }],
            span: [summary.low, summary.high],
            cells: [
              times(summary.ratio),
              `${times(summary.low)}–${times(summary.high)}`,
              `${summary.aaLow.toFixed(3)}–${summary.aaHigh.toFixed(3)}`,
            ],
          }),
        ),
      },
    ];
    const bounds = groups.flatMap((group) => group.rows.flatMap((row) => row.span!));
    const scale = speedScale(bounds, "");
    scale.ticks = scale.ticks.map((tick) =>
      tick.value === 1 ? { ...tick, label: "Tessera" } : tick,
    );
    const title = `${recordType(input.ty).label} · ${input.distribution} · ${formatLength(input.len)} records`;
    return [
      {
        name: `selected-${id.replaceAll("/", "-").replaceAll(".", "_")}`,
        subject: title,
        summary: "Each sort's time relative to the paired Tessera control, with its block range.",
        title,
        ...rendered({
          title,
          guide: `Sort time / paired Tessera time · ${dataset.manifest.blocks} paired blocks`,
          columns: ["Time ratio", "Block range", "A/A range"],
          groups,
          scale,
          legend: {
            note: "dot: median · line: block range · triangle: off-scale · paired Tessera control",
          },
          axis: ["← less time", "more time →"],
        }),
        table: markdownTable(
          ["Build", "Sort", "Time ratio", "Block range", "A/A range"],
          groups.flatMap((group) =>
            group.rows.map((row) => [group.title, row.name.input, ...row.cells]),
          ),
        ),
      },
    ];
  });
}

/** The release comparison's cohort: the catalog's baseline patterns and lengths. */
const RELEASE: Cohort = {
  ranking: {
    ty: "u64",
    n: LARGE,
    lengths,
    distributions,
    label: `geometric mean over the ${distributions.length} baseline input patterns`,
  },
  n: LARGE,
  sweep: { distributionId: "random", lengths },
  short: { lengths: [SHORT], types: ["u64"] },
  distributions,
  typeDistributions: distributions,
  typeDistributionsLabel: `geometric mean over the ${distributions.length} baseline input patterns`,
};

/** One figure for a first impression: the ranking with each sort's extra memory. */
/**
 * Every comparison figure the field supports for one cohort, in reading order. With
 * `memory`, the ranking adds an extra-memory column and becomes the `overview` figure.
 */
export function comparisonFigures(
  field: Field,
  cohort: Cohort,
  memory = false,
  growth?: Growth,
): Figure[] {
  return [
    renderRanking(field, cohort, memory),
    renderLengths(field, cohort),
    renderInputs(field, cohort, "inputs"),
    ...catalog.types
      .map((type) => type.id)
      .filter((ty) => ty !== "u64")
      .map((ty) => renderInputs(field, cohort, `inputs-${ty}`, ty)),
    renderShort(field, cohort),
    renderTypes(field, cohort),
    renderGrowth(growth ?? fieldGrowth(field, "u64")),
  ].filter((f): f is Figure => f !== null);
}

export function renderFigures(dataset: Dataset, summaries: Map<string, Summary>): Figure[] {
  const field = reportField(dataset, summaries);
  // Release figures need the release ranking's cases. A smaller selection shows each of its
  // cases instead, so no case loses its paired ranges to a partial release panel.
  const release = rankingCases(terms(SORT), RELEASE, RELEASE.ranking.n).every((id) =>
    field.cases.some((input) => caseId(input) === id),
  );
  return release ? comparisonFigures(field, RELEASE) : renderSelected(dataset, summaries);
}
