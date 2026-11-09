// SPDX-License-Identifier: BSD-2-Clause

import { algorithm, catalog, recordType } from "../catalog.ts";
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
  title: string;
  svg: string;
  table: string;
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
// small difference does not print as "1×".
const times = (v: number) =>
  `${v >= 100 ? v.toFixed(0) : v < 1.095 ? v.toFixed(2) : Number(v.toPrecision(2))}×`;
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
function speedOnCase(
  summaries: Map<string, Summary>,
  id: string,
  variant: string,
  build = "default",
) {
  const std = summaries.get(measurementId(build, id, "std-stable")),
    s = summaries.get(measurementId(build, id, variant));
  if (!std || !s) return null;
  return variant === "tessera" ? std.ratio : median(std.ratios.map((r, i) => r / s.ratios[i]));
}

// The geometric mean of those speeds over cases, or null if one is missing.
function speedOver(
  summaries: Map<string, Summary>,
  ids: string[],
  variant: string,
  build = "default",
) {
  const values = ids.map((id) => speedOnCase(summaries, id, variant, build));
  return values.every((v): v is number => v !== null) ? geomean(values) : null;
}
const u64Cases = (n: number) =>
  shapes.map((shape) => caseId({ ty: "u64", distribution: shape, len: n }));

const kindLegend = (simd: boolean): [Swatch[], string][] => [
  [["primary"], "Tessera"],
  ...(simd ? [[["alternate"], "Tessera, Portable SIMD"] as [Swatch[], string]] : []),
  [["radix"], "other radix sorts"],
  [["comparison"], "comparison sorts"],
];
const SPEED_AXIS: [string, string] = ["← slower", "faster →"];

function renderRanking(dataset: Dataset, summaries: Map<string, Summary>): Figure | null {
  const entries = dataset.manifests.get("default")!.variants.map((v) => ({
    cell: sortName(v.name),
    kind: family(v.name),
    tone: toneOf(v.name),
    simd: false,
    stable: v.stable,
    speeds: lengths.map((n) => speedOver(summaries, u64Cases(n), v.name)),
  }));
  if (dataset.manifests.has("portable-simd"))
    entries.push({
      cell: { input: "Tessera, Portable SIMD", sub: "", tag: "", strong: true },
      kind: "radix sort, SIMD build",
      tone: "primary" as Tone,
      simd: true,
      stable: true,
      speeds: lengths.map((n) => speedOver(summaries, u64Cases(n), "tessera", "portable-simd")),
    });
  const at = lengths.indexOf(LARGE);
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
    marks: [{ value: e.speeds[at], tone: e.tone, hollow: e.simd }],
    cells: [times(e.speeds[at])],
    strong: e.cell.strong,
  });
  const field = `${others.length + 1} sorts on ${formatLength(LARGE)} u64 keys`;
  const title =
    rank === 1
      ? `Tessera is the fastest of ${field}`
      : `Tessera ranks ${ordinal(rank)} of ${field}`;
  return {
    name: "ranking",
    title,
    svg: renderRows({
      title,
      guide: "Speed relative to slice::sort, geometric mean over the 10 baseline input patterns",
      columns: ["Speed"],
      groups: [
        { title: "Stable sorts", rows: complete.filter((e) => e.stable).map(row) },
        { title: "Unstable sorts", rows: complete.filter((e) => !e.stable).map(row) },
      ].filter((g) => g.rows.length),
      scale: speedScale(complete.map((e) => e.speeds[at])),
      axis: SPEED_AXIS,
      legend: kindLegend(complete.some((e) => e.simd)),
    }),
    table: markdownTable(
      [
        "Sort",
        "Kind",
        "Stability",
        ...lengths.map((n) => `${formatLength(n)} keys, speed vs slice::sort`),
      ],
      complete.map((e) => [
        e.cell.input,
        e.kind,
        e.stable ? "stable" : "unstable",
        ...e.speeds.map(times),
      ]),
    ),
  };
}

interface Standing {
  speeds: { variant: string; speed: number }[];
  tessera: number;
  // The Portable SIMD build, anchored to slice::sort in its own process.
  simd: number | null;
  rank: number;
}
// Where Tessera stands among every eligible sort on one row, all measured
// against slice::sort, which therefore sits at the origin and is not drawn.
function standing(
  summaries: Map<string, Summary>,
  variants: { name: string; stable: boolean }[],
  ids: string[],
  stableOnly: boolean,
): Standing | null {
  const speedOf = (variant: string) => speedOver(summaries, ids, variant);
  const tessera = speedOf("tessera");
  if (tessera === null) return null;
  const speeds = variants
    .filter((v) => v.name !== "tessera" && (!stableOnly || v.stable))
    .map((v) => ({ variant: v.name, speed: speedOf(v.name) }))
    .filter((v): v is { variant: string; speed: number } => v.speed !== null);
  return {
    speeds,
    tessera,
    simd: speedOver(summaries, ids, "tessera", "portable-simd"),
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
      ...(s.simd === null ? [] : [{ value: s.simd, tone: "primary" as Tone, hollow: true }]),
      { value: s.tessera, tone: "primary" },
    ],
    cells: [times(s.tessera), ordinal(s.rank)],
    strong: true,
  };
}

// The finding of a field figure, stated as its title.
function fieldTitle(standings: Standing[], unit: string) {
  const total = standings.length,
    first = standings.filter((s) => s.rank === 1).length,
    top3 = standings.filter((s) => s.rank <= 3).length;
  if (first === total) return `Tessera is fastest on all ${total} ${unit}`;
  const top = top3 > first ? `, top 3 on ${top3}` : "";
  return `Tessera is fastest on ${first} of ${total} ${unit}${top}`;
}

function fieldTable(rows: [string, Standing][], nsPerKey?: number[]) {
  const simd = rows.some(([, s]) => s.simd !== null);
  return markdownTable(
    [
      "Row",
      ...(nsPerKey ? ["Tessera time per key"] : []),
      "Tessera rank",
      "Tessera speed vs slice::sort",
      ...(simd ? ["Portable SIMD speed"] : []),
      "Other sorts, fastest first",
    ],
    rows.map(([label, s], i) => [
      label,
      ...(nsPerKey ? [formatNs(nsPerKey[i])] : []),
      `${ordinal(s.rank)} of ${s.speeds.length + 1}`,
      times(s.tessera),
      ...(simd ? [s.simd === null ? "—" : times(s.simd)] : []),
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
  dataset: Dataset,
  summaries: Map<string, Summary>,
  name: string,
  n: number,
  ty = "u64",
): Figure | null {
  const manifest = dataset.manifests.get("default")!;
  const variants = manifest.variants;
  const patterns = manifest.cases
    .filter((input) => input.ty === ty && input.len === n)
    .map((input) => input.distribution);
  // Retain the small-selection fallback rather than presenting a single input
  // as a full pattern panel. Non-u64 panels add the type-specific coverage.
  if (ty === "u64" && !shapes.every((shape) => patterns.includes(shape))) return null;
  if (ty !== "u64" && !patterns.some((shape) => !shapes.includes(shape))) return null;
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
        const s = standing(summaries, variants, [id], stableOnly);
        const ns = summaries.get(measurementId("default", id, "tessera"))?.ns;
        if (!s || ns === undefined) return [];
        plotted.push([label, s]);
        nsPerKey.push(ns);
        return [fieldRow({ input: label, sub: "", tag: "" }, s)];
      }),
  })).filter((g) => g.rows.length);
  if (!plotted.length) return null;
  const standings = plotted.map(([, s]) => s);
  const title = fieldTitle(
    standings,
    ty === "u64" ? "input patterns" : `${record.label} input patterns`,
  );
  return {
    name,
    title,
    svg: renderRows({
      title,
      guide: `${formatLength(n)} ${ty === "u64" ? "u64 keys" : record.label}; each dot is one ${stableOnly ? "stable " : ""}sort, placed by its speed relative to slice::sort`,
      columns: ["Speed", "Rank"],
      groups,
      scale: speedScale(
        standings.flatMap((s) => [s.tessera, s.simd ?? s.tessera, ...s.speeds.map((v) => v.speed)]),
      ),
      axis: SPEED_AXIS,
      legend: kindLegend(standings.some((s) => s.simd !== null)),
    }),
    table: fieldTable(plotted, nsPerKey),
  };
}

// Each element type as a geometric mean over the ten inputs. Records admit
// only stable sorts, because stability is observable there.
function renderTypes(dataset: Dataset, summaries: Map<string, Summary>): Figure | null {
  const variants = dataset.manifests.get("default")!.variants;
  const plotted: [string, Standing][] = [];
  const groups = TYPE_GROUPS.map((g) => ({
    title: g.title,
    rows: g.types.flatMap(([ty, label, sub]) => {
      const ids = shapes.map((shape) => caseId({ ty, distribution: shape, len: LARGE }));
      const s = standing(summaries, variants, ids, !g.keys);
      if (!s) return [];
      plotted.push([label, s]);
      return [fieldRow({ input: label, sub, tag: "" }, s)];
    }),
  })).filter((g) => g.rows.length);
  if (!plotted.length) return null;
  const standings = plotted.map(([, s]) => s);
  const title = fieldTitle(standings, "element types");
  return {
    name: "types",
    title,
    svg: renderRows({
      title,
      guide: `${formatLength(LARGE)} elements; speed relative to slice::sort, geometric mean over the 10 baseline input patterns`,
      columns: ["Speed", "Rank"],
      groups,
      scale: speedScale(
        standings.flatMap((s) => [s.tessera, s.simd ?? s.tessera, ...s.speeds.map((v) => v.speed)]),
      ),
      axis: SPEED_AXIS,
      legend: kindLegend(standings.some((s) => s.simd !== null)),
    }),
    table: fieldTable(plotted),
  };
}

function renderMemory(dataset: Dataset): Figure | null {
  const n = LARGEST;
  const patterns = dataset.manifests
    .get("default")!
    .cases.filter((input) => input.ty === "u64" && input.len === n)
    .map((input) => input.distribution);
  if (!patterns.includes("random")) return null;
  const peaks = (variant: string) => {
    const found = patterns.map(
      (shape) =>
        dataset.memory.find(
          (m) =>
            m.build === "default" &&
            m.variant === variant &&
            m.case_id === caseId({ ty: "u64", distribution: shape, len: n }),
        )?.peak_extra_bytes,
    );
    return found.every((v): v is number => v !== undefined)
      ? { random: found[patterns.indexOf("random")], worst: Math.max(...found) }
      : null;
  };
  const entries = dataset.manifests
    .get("default")!
    .variants.map((v) => ({ ...v, peak: peaks(v.name)! }))
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
  const title = `Tessera sorts ${formatLength(n)} u64 keys in at most ${formatBytes(anchor.peak.worst)} of extra memory`;
  return {
    name: "memory",
    title,
    svg: renderRows({
      title,
      guide: `Peak heap allocation during one sort, beyond the ${formatBytes(input)} input`,
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
      legend: [...kindLegend(false), [["hollow"], "random input"], [["reference"], "input size"]],
    }),
    table: markdownTable(["Sort", "Random input", `Worst of ${patterns.length} inputs`], plotted),
  };
}

/** Plot selected inputs when a plan does not contain a complete release panel. */
function renderSelected(dataset: Dataset, summaries: Map<string, Summary>): Figure[] {
  return dataset.manifests.get("default")!.cases.flatMap((input) => {
    const id = caseId(input);
    const groups = [...dataset.manifests.keys()].flatMap((build) => {
      const entries = dataset.manifests.get(build)!.variants.flatMap((variant) => {
        const summary = summaries.get(measurementId(build, id, variant.name));
        return summary ? [summary] : [];
      });
      if (!entries.length) return [];
      return [
        {
          title: build,
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
    });
    if (!groups.length) return [];
    const bounds = groups.flatMap((group) => group.rows.flatMap((row) => row.span!));
    const scale = speedScale(bounds);
    scale.ticks = scale.ticks.map((tick) =>
      tick.value === 1 ? { ...tick, label: "Tessera" } : tick,
    );
    const title = `${recordType(input.ty).label} · ${input.distribution} · ${formatLength(input.len)} records`;
    return [
      {
        name: `selected-${id.replaceAll("/", "-").replaceAll(".", "_")}`,
        title,
        svg: renderRows({
          title,
          guide: "Sort time / paired Tessera time · nine blocks per build",
          columns: ["Time ratio", "Block range", "A/A range"],
          groups,
          scale,
          legend: {
            note: "dot: median · line: block range · triangle: off-scale · per-build Tessera control",
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

export function renderFigures(dataset: Dataset, summaries: Map<string, Summary>): Figure[] {
  const figures = [
    renderRanking(dataset, summaries),
    renderInputs(dataset, summaries, "inputs", LARGE),
    ...catalog.comparison.types
      .filter((ty) => ty !== "u64")
      .map((ty) => renderInputs(dataset, summaries, `inputs-${ty}`, LARGE, ty)),
    renderInputs(dataset, summaries, "short", SHORT),
    renderTypes(dataset, summaries),
    renderMemory(dataset),
  ].filter((f): f is Figure => f !== null);
  return figures.length ? figures : renderSelected(dataset, summaries);
}
