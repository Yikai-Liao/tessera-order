// SPDX-License-Identifier: BSD-2-Clause

import { algorithm, type CaseIdentity, catalog, recordType } from "../catalog.ts";
import type { NameCell } from "../plot/layout.ts";
import {
  type RowChart as Chart,
  type Mark,
  type PlotRow as Row,
  renderRows,
  type Swatch,
  type Tone,
} from "../plot/rows.ts";
import { geomean, median } from "../statistics.ts";
import { caseId, type Dataset, lengths, measurementId, type Summary, shapes } from "./model.ts";

export interface Figure {
  name: string;
  /** What the figure compares, as a section heading: "u64 keys by input pattern". */
  subject: string;
  /** One sentence on what is compared and how to read it, shown before the figure. */
  summary: string;
  /** The finding, as the chart's own title. */
  title: string;
  /** The chart description, for callers that render it themselves, such as the site. */
  chart: Chart;
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
  algorithms: { name: string; stable: boolean }[];
  cases: CaseIdentity[];
  /** slice::sort time over the algorithm's time on one case, or null if either is missing. */
  speed(id: string, algorithm: string): number | null;
  /** Nanoseconds per element, or null if unmeasured. */
  nsPerKey(id: string, algorithm: string): number | null;
  /** Peak extra heap bytes of one sort, or null if unmeasured. */
  peak(id: string, algorithm: string): number | null;
}

/** The inputs a figure set summarizes and how its guides name them. */
export interface Cohort {
  /**
   * The ranking's element type, length and input patterns. Lengths are tabulated next to
   * the ranking, which admits sorts measured at all of them; `label` follows
   * "speed vs slice::sort, " in its guide.
   */
  ranking: { ty: string; n: number; lengths: number[]; shapes: string[]; label: string };
  /** Length of the input-pattern and element-type figures. */
  n: number;
  /** Input patterns every u64 input-pattern figure must have. */
  shapes: string[];
  /** Input patterns the element-type figure compares; every type must have each of them. */
  typeShapes: string[];
  /** How the element-type figure combines them, after "speed relative to slice::sort, ". */
  typeShapesLabel: string;
  /** Length of the memory figure. */
  memoryN: number;
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

const displayName = (id: string) => algorithm(id).label;
const family = (id: string) => algorithm(id).description ?? `${algorithm(id).family} sort`;
const sortName = (id: string): NameCell => ({
  input: displayName(id),
  sub: algorithm(id).detail ?? "",
  tag: "",
  strong: id === "tessera",
});
const SHAPE_GROUPS = [
  ...Map.groupBy(
    catalog.distributions.filter((shape) => shape.group),
    (shape) => shape.group!,
  ),
].map(([title, entries]) => ({
  title,
  shapes: entries.map((shape) => [shape.id, shape.label] as const),
}));
const TYPE_GROUPS = catalog.comparison.typeGroups.map((group) => ({
  ...group,
  types: group.types.map((id) => {
    const type = recordType(id);
    return [id, type.label, type.detail] as const;
  }),
}));

const LARGE = 1000000;
const SHORT = 1000;
const LARGEST = 10000000;

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
    : algorithm(variant).family === "radix"
      ? "radix"
      : "comparison";
// A symmetric speed axis in doublings around slice::sort, wide enough for
// the data. Left of the centre the labels are fractions, so a tick and a
// value cell read the same way.
function speedScale(ratios: number[]): Chart["scale"] {
  const reach = Math.max(2, ...ratios.map((r) => Math.min(RATIO_LIMIT, Math.max(r, 1 / r)) * 1.08));
  const span = Math.min(RATIO_LIMIT, 2 ** Math.ceil(Math.log2(reach)));
  const steps = Math.log2(span);
  // Every doubling is labelled while there is room; wider axes label every other one.
  const every = steps <= 3 ? 1 : 2;
  const ticks = [{ value: 1, label: "slice::sort" }];
  for (let k = every; k <= steps; k += every) {
    ticks.push({ value: 2 ** k, label: `${2 ** k}×` }, { value: 2 ** -k, label: `1/${2 ** k}×` });
  }
  return { low: 1 / span, high: span, ticks, origin: 1, clamp: true };
}

// Speed of one variant relative to slice::sort in the same blocks: the
// slice::sort time over the variant's, per block, then the median.
function pairedSpeed(summaries: Map<string, Summary>, id: string, variant: string) {
  const std = summaries.get(measurementId("default", id, "std-stable")),
    s = summaries.get(measurementId("default", id, variant));
  if (!std || !s) return null;
  return variant === "tessera" ? std.ratio : median(std.ratios.map((r, i) => r / s.ratios[i]));
}

/** A local report's field: paired speeds and the memory worker's peaks. */
export function reportField(dataset: Dataset, summaries: Map<string, Summary>): Field {
  return {
    algorithms: dataset.manifest.variants,
    cases: dataset.manifest.cases,
    speed: (id, variant) => pairedSpeed(summaries, id, variant),
    nsPerKey: (id, variant) => summaries.get(measurementId("default", id, variant))?.ns ?? null,
    peak: (id, variant) =>
      dataset.memory.find((m) => m.build === "default" && m.variant === variant && m.case_id === id)
        ?.peak_extra_bytes ?? null,
  };
}

// The geometric mean of those speeds over cases, or null if one is missing.
function speedOver(field: Field, ids: string[], variant: string) {
  const values = ids.map((id) => field.speed(id, variant));
  return values.every((v): v is number => v !== null) ? geomean(values) : null;
}
const rankingCases = ({ ranking }: Cohort, n: number) =>
  ranking.shapes.map((shape) => caseId({ ty: ranking.ty, distribution: shape, len: n }));
/** Elements of one type in a guide: "u64 keys" or "{u32, u32} records". */
const elements = (ty: string) => `${recordType(ty).label} ${recordType(ty).task}`;

const kindLegend: [Swatch[], string][] = [
  [["primary"], "Tessera"],
  [["radix"], "other radix sorts"],
  [["comparison"], "comparison sorts"],
];
const SPEED_AXIS: [string, string] = ["← slower", "faster →"];

// With `memory`, each sort also shows its largest peak over the ranked inputs,
// so one figure states both speed and memory.
function renderRanking(field: Field, cohort: Cohort, memory = false): Figure | null {
  const worstPeak = (variant: string) => {
    const peaks = rankingCases(cohort, cohort.ranking.n).map((id) => field.peak(id, variant));
    return peaks.every((v): v is number => v !== null) ? Math.max(...peaks) : null;
  };
  const entries = field.algorithms.map((v) => ({
    cell: sortName(v.name),
    kind: family(v.name),
    tone: toneOf(v.name),
    stable: v.stable,
    speeds: cohort.ranking.lengths.map((n) => speedOver(field, rankingCases(cohort, n), v.name)),
    peak: memory ? worstPeak(v.name) : null,
  }));
  const at = cohort.ranking.lengths.indexOf(cohort.ranking.n);
  const complete = entries
    .filter((e) => e.speeds.every((v) => v !== null))
    .map((e) => ({ ...e, speeds: e.speeds as number[] }))
    .sort((a, b) => b.speeds[at] - a.speeds[at]);
  const anchor = complete.find((e) => e.cell.input === "Tessera");
  if (!anchor) return null;
  const others = complete.filter((e) => !e.cell.strong);
  const rank = 1 + others.filter((e) => e.speeds[at] > anchor.speeds[at]).length;
  const row = (e: (typeof complete)[number]): Row => ({
    name: e.cell,
    marks: [{ value: e.speeds[at], tone: e.tone }],
    cells: [times(e.speeds[at]), ...(memory ? [e.peak === null ? "–" : formatBytes(e.peak)] : [])],
    strong: e.cell.strong,
  });
  const sorts = `${others.length + 1} sorts`;
  const title = rank === 1 ? `Fastest of ${sorts}` : `Ranked ${ordinal(rank)} of ${sorts}`;
  return {
    name: memory ? "overview" : "ranking",
    subject: "Overall ranking",
    summary: `Every sort on the same ${elements(cohort.ranking.ty)}, ranked by speed relative to slice::sort${memory ? ", with the largest extra memory it allocated" : ""}. Stable and unstable sorts are grouped separately.`,
    title,
    ...rendered({
      title,
      guide: `${formatLength(cohort.ranking.n)} ${elements(cohort.ranking.ty)} · speed vs slice::sort, ${cohort.ranking.label}`,
      columns: ["Speed", ...(memory ? ["Extra memory"] : [])],
      groups: [
        { title: "Stable sorts", rows: complete.filter((e) => e.stable).map(row) },
        { title: "Unstable sorts", rows: complete.filter((e) => !e.stable).map(row) },
      ].filter((g) => g.rows.length),
      scale: speedScale(complete.map((e) => e.speeds[at])),
      axis: SPEED_AXIS,
      legend: kindLegend,
    }),
    table: markdownTable(
      [
        "Sort",
        "Kind",
        "Stability",
        ...cohort.ranking.lengths.map((n) => `${formatLength(n)} elements, speed vs slice::sort`),
        ...(memory ? ["Largest extra memory"] : []),
      ],
      complete.map((e) => [
        e.cell.input,
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
// Where Tessera stands among every eligible sort on one row, all measured
// against slice::sort, which therefore sits at the origin and is not drawn.
function standing(field: Field, ids: string[], stableOnly: boolean): Standing | null {
  const variants = field.algorithms;
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

function fieldRow(name: NameCell, s: Standing): Row {
  return {
    name,
    swarm: true,
    marks: [
      ...s.speeds
        .filter((v) => v.variant !== "std-stable")
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
  if (first === total) return `Fastest on all ${total} ${unit}`;
  // A podium needs more than three sorts in the field.
  const field = Math.max(...standings.map((s) => s.speeds.length + 1));
  const podium = top3 > first && field > 3;
  // Without a first place, the podium is the finding.
  if (!first && podium)
    return top3 === total
      ? `Top 3 on all ${total} ${unit}`
      : `Top 3 on ${top3} of ${total} ${unit}`;
  return `Fastest on ${first} of ${total} ${unit}${podium ? `, top 3 on ${top3}` : ""}`;
}

function fieldTable(rows: [string, Standing][], nsPerKey?: number[]) {
  return markdownTable(
    [
      "Row",
      ...(nsPerKey ? ["Tessera time per key"] : []),
      "Tessera rank",
      "Tessera speed vs slice::sort",
      "Other sorts, fastest first",
    ],
    rows.map(([label, s], i) => [
      label,
      ...(nsPerKey ? [formatNs(nsPerKey[i])] : []),
      `${ordinal(s.rank)} of ${s.speeds.length + 1}`,
      times(s.tessera),
      [...s.speeds]
        .sort((a, b) => b.speed - a.speed)
        .map((v) => `${displayName(v.variant)} ${times(v.speed)}`)
        .join(", "),
    ]),
  );
}

// Each selected input pattern of one type. Records admit only stable sorts;
// for keys, stability is unobservable and every sort competes.
function renderInputs(
  field: Field,
  cohort: Cohort,
  name: string,
  n: number,
  ty = "u64",
): Figure | null {
  const patterns = field.cases
    .filter((input) => input.ty === ty && input.len === n)
    .map((input) => input.distribution);
  // Retain the small-selection fallback rather than presenting a single input
  // as a full pattern panel. Non-u64 panels add the type-specific coverage.
  if (ty === "u64" && !cohort.shapes.every((shape) => patterns.includes(shape))) return null;
  if (ty !== "u64" && !patterns.some((shape) => !cohort.shapes.includes(shape))) return null;
  const record = recordType(ty);
  const stableOnly = record.task === "records";
  const plotted: [string, Standing][] = [];
  const nsPerKey: number[] = [];
  const groups = SHAPE_GROUPS.map((g) => ({
    title: g.title,
    rows: g.shapes
      .filter(([shape]) => patterns.includes(shape))
      .flatMap(([shape, label]) => {
        const id = caseId({ ty, distribution: shape, len: n });
        const s = standing(field, [id], stableOnly);
        const ns = field.nsPerKey(id, "tessera");
        if (!s || ns === null) return [];
        plotted.push([label, s]);
        nsPerKey.push(ns);
        return [fieldRow({ input: label, sub: "", tag: "" }, s)];
      }),
  })).filter((g) => g.rows.length);
  if (!plotted.length) return null;
  const standings = plotted.map(([, s]) => s);
  const title = fieldTitle(standings, "inputs");
  return {
    name,
    subject: `${elements(ty)} by input pattern${n <= SHORT ? `, ${formatLength(n)} elements` : ""}`,
    summary: `Each row is one input pattern. Each dot is one ${stableOnly ? "stable sort (records keep equal keys in order)" : "sort"}, placed by its speed relative to slice::sort; the columns give Tessera's speed and rank.`,
    title,
    ...rendered({
      title,
      guide: `${formatLength(n)} ${elements(ty)} · each dot is one ${stableOnly ? "stable " : ""}sort, placed by its speed relative to slice::sort`,
      columns: ["Speed", "Rank"],
      groups,
      scale: speedScale(standings.flatMap((s) => [s.tessera, ...s.speeds.map((v) => v.speed)])),
      axis: SPEED_AXIS,
      legend: kindLegend,
    }),
    table: fieldTable(plotted, nsPerKey),
  };
}

// Each element type over the cohort's type patterns. Records admit
// only stable sorts, because stability is observable there.
function renderTypes(field: Field, cohort: Cohort): Figure | null {
  const plotted: [string, Standing][] = [];
  const groups = TYPE_GROUPS.map((g) => ({
    title: g.title,
    rows: g.types.flatMap(([ty, label, sub]) => {
      const ids = cohort.typeShapes.map((shape) =>
        caseId({ ty, distribution: shape, len: cohort.n }),
      );
      const s = standing(field, ids, !g.keys);
      if (!s) return [];
      plotted.push([label, s]);
      return [fieldRow({ input: label, sub, tag: "" }, s)];
    }),
  })).filter((g) => g.rows.length);
  // A comparison of element types needs at least two of them.
  if (plotted.length < 2) return null;
  const standings = plotted.map(([, s]) => s);
  const title = fieldTitle(standings, "types");
  return {
    name: "types",
    subject: "Element types",
    summary:
      "Each row is one key or record type. Each dot is one sort, placed by its speed relative to slice::sort; records compare only stable sorts.",
    title,
    ...rendered({
      title,
      guide: `${formatLength(cohort.n)} elements · speed relative to slice::sort, ${cohort.typeShapesLabel}`,
      columns: ["Speed", "Rank"],
      groups,
      scale: speedScale(standings.flatMap((s) => [s.tessera, ...s.speeds.map((v) => v.speed)])),
      axis: SPEED_AXIS,
      legend: kindLegend,
    }),
    table: fieldTable(plotted),
  };
}

function renderMemory(field: Field, cohort: Cohort): Figure | null {
  const n = cohort.memoryN;
  const patterns = field.cases
    .filter((input) => input.ty === "u64" && input.len === n)
    .map((input) => input.distribution);
  if (!patterns.includes("random")) return null;
  const peaks = (variant: string) => {
    const found = patterns.map((shape) =>
      field.peak(caseId({ ty: "u64", distribution: shape, len: n }), variant),
    );
    return found.every((v): v is number => v !== null)
      ? { random: found[patterns.indexOf("random")], worst: Math.max(...found) }
      : null;
  };
  const entries = field.algorithms
    .map((v) => ({ ...v, peak: peaks(v.name)! }))
    .filter((e) => e.peak)
    .sort((a, b) => a.peak.worst - b.peak.worst);
  const anchor = entries.find((e) => e.name === "tessera");
  if (!anchor) return null;
  const low = 1e5,
    high = 1e9;
  const plotted: string[][] = [];
  const row = (e: (typeof entries)[number]): Row => {
    const { random, worst } = e.peak;
    plotted.push([displayName(e.name), formatBytes(random), formatBytes(worst)]);
    const tone = toneOf(e.name);
    // The random-input value is the ring; only the worst case is a number.
    const cells = [worst ? formatBytes(worst) : "0"];
    if (!worst) return { name: sortName(e.name), marks: [], cells };
    const spread = random !== worst;
    return {
      name: sortName(e.name),
      marks: [
        ...(spread ? [{ value: Math.max(low, random), tone, hollow: true }] : []),
        { value: worst, tone },
      ],
      span: spread ? [Math.max(low, random), worst] : undefined,
      cells,
      strong: tone === "primary",
    };
  };
  const input = n * 8;
  const title = `At most ${formatBytes(anchor.peak.worst)} of extra memory`;
  return {
    name: "memory",
    subject: "Extra memory",
    summary:
      "The largest heap allocation each sort makes beyond its input. The ring is the random input, the dot the worst input; the dashed line is the size of the input itself.",
    title,
    ...rendered({
      title,
      guide: `${formatLength(n)} ${elements("u64")} · peak heap allocation during one sort, beyond the ${formatBytes(input)} input`,
      columns: ["Worst"],
      groups: [
        { title: "Stable sorts", rows: entries.filter((e) => e.stable).map(row) },
        { title: "Unstable sorts", rows: entries.filter((e) => !e.stable).map(row) },
      ].filter((g) => g.rows.length),
      scale: {
        low,
        high,
        clamp: false,
        ticks: [1e5, 1e6, 1e7, 1e8, 1e9].map((v) => ({ value: v, label: formatBytes(v) })),
      },
      marker: { value: input, label: "input size" },
      axis: ["← less memory", "more memory →"],
      legend: [...kindLegend, [["hollow"], "random input"], [["reference"], "input size"]],
    }),
    table: markdownTable(["Sort", "Random input", `Worst of ${patterns.length} inputs`], plotted),
  };
}

/**
 * Tessera's parallel entry point and Rayon's stable sort at each thread count, as speed
 * relative to Rayon's stable sort on one thread, each a geometric mean over the same inputs.
 */
export interface Scaling {
  n: number;
  inputs: number;
  rows: { threads: number; tessera: number; rayon: number }[];
}

export function renderParallel({ n, inputs, rows }: Scaling): Figure | null {
  if (!rows.length) return null;
  const threads = (t: number) => `${t} thread${t === 1 ? "" : "s"}`;
  const widest = rows.at(-1)!;
  const title = `${times(widest.tessera / widest.rayon)} Rayon's speed on ${threads(widest.threads)}`;
  const scale = speedScale(rows.flatMap((row) => [row.tessera, row.rayon]));
  scale.ticks = scale.ticks.map((tick) =>
    tick.value === 1 ? { ...tick, label: "Rayon · 1" } : tick,
  );
  return {
    name: "parallel",
    subject: "Parallel sorting",
    summary:
      "Tessera's Rayon entry point against Rayon's own stable par_sort at each thread count, both as speed relative to Rayon on one thread.",
    title,
    ...rendered({
      title,
      guide: `${formatLength(n)} elements · geometric mean over the ${inputs} inputs both measured at every thread count`,
      columns: ["Tessera", "Rayon"],
      groups: [
        {
          title: "Threads",
          rows: rows.map(
            (row): Row => ({
              name: { input: threads(row.threads), sub: "", tag: "" },
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
      ["Threads", "Tessera speed vs Rayon on 1 thread", "Rayon speed vs Rayon on 1 thread"],
      rows.map((row) => [String(row.threads), times(row.tessera), times(row.rayon)]),
    ),
  };
}

/** Plot selected inputs when a plan does not contain a complete release panel. */
function renderSelected(dataset: Dataset, summaries: Map<string, Summary>): Figure[] {
  return dataset.manifest.cases.flatMap((input) => {
    const id = caseId(input);
    const entries = dataset.manifest.variants.flatMap((variant) => {
      const summary = summaries.get(measurementId("default", id, variant.name));
      return summary ? [summary] : [];
    });
    if (!entries.length) return [];
    const groups = [
      {
        title: `Algorithms · ${dataset.manifest.blocks} blocks`,
        rows: entries.map(
          (summary): Row => ({
            name: sortName(summary.variant),
            marks: [{ value: summary.ratio, tone: toneOf(summary.variant) }],
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
    const scale = speedScale(bounds);
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
    shapes,
    label: `geometric mean over the ${shapes.length} baseline input patterns`,
  },
  n: LARGE,
  shapes,
  typeShapes: shapes,
  typeShapesLabel: `geometric mean over the ${shapes.length} baseline input patterns`,
  memoryN: LARGEST,
};

/** One figure for a first impression: the ranking with each sort's extra memory. */
/**
 * Every comparison figure the field supports for one cohort, in reading order. With
 * `memory`, the ranking adds an extra-memory column and becomes the `overview` figure.
 */
export function comparisonFigures(
  field: Field,
  cohort: Cohort,
  short?: number,
  memory = false,
): Figure[] {
  return [
    renderRanking(field, cohort, memory),
    renderInputs(field, cohort, "inputs", cohort.n),
    ...catalog.comparison.types
      .filter((ty) => ty !== "u64")
      .map((ty) => renderInputs(field, cohort, `inputs-${ty}`, cohort.n, ty)),
    ...(short === undefined ? [] : [renderInputs(field, cohort, "short", short)]),
    renderTypes(field, cohort),
    renderMemory(field, cohort),
  ].filter((f): f is Figure => f !== null);
}

export function renderFigures(dataset: Dataset, summaries: Map<string, Summary>): Figure[] {
  const figures = comparisonFigures(reportField(dataset, summaries), RELEASE, SHORT);
  return figures.length ? figures : renderSelected(dataset, summaries);
}
