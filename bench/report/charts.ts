// SPDX-License-Identifier: BSD-2-Clause

import {
  drawName,
  type NameCell,
  nameWidth,
  svgWriter,
  type Text,
} from "../../.github/scripts/bencher/chart.ts";
import { chartStyles, escapeXml, textWidth } from "../../.github/scripts/bencher/svg.ts";
import {
  caseId,
  type Dataset,
  geomean,
  lengths,
  measurementId,
  median,
  type Summary,
  shapes,
} from "./model.ts";

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

const PAD = 24;
const W = 720;
// Rows with a second line under the name need the taller height.
const ROW_H = 38;
const ROW_H_SINGLE = 34;
const HEADER_RULE = 76;
const PLOT_TOP = HEADER_RULE + 14;
// Losses on short skewed inputs reach 50× against SIMD sorts. Beyond 32× a
// mark sits on the edge as a triangle and the value keeps the exact figure.
const RATIO_LIMIT = 32;

const NAMES: Record<string, string> = {
  tessera: "Tessera",
  "std-stable": "slice::sort",
  "std-unstable": "slice::sort_unstable",
};
const RADIX = new Set(["radsort", "voracious", "rdst", "ips2ra", "clausecker"]);
const SIMD = new Set(["vqsort", "x86simdsort"]);
const displayName = (variant: string) => NAMES[variant] ?? variant;
const family = (variant: string) =>
  variant === "glidesort-sqrt"
    ? "comparison sort with Tessera's memory"
    : variant === "tessera" || RADIX.has(variant)
      ? "radix sort"
      : SIMD.has(variant)
        ? "SIMD sort"
        : "comparison sort";

// The one sort whose name needs a second line in a figure: glidesort-sqrt
// receives Tessera's measured peak as its buffer, so its memory matches
// Tessera's by construction.
const sortName = (variant: string): NameCell => ({
  input: displayName(variant),
  sub: variant === "glidesort-sqrt" ? "given Tessera's memory" : "",
  tag: "",
  strong: variant === "tessera",
});

const SHAPE_GROUPS: { title: string; shapes: [string, string][] }[] = [
  { title: "Uniform", shapes: [["random", "random"]] },
  {
    title: "Few distinct keys",
    shapes: [
      ["d256", "256 values"],
      ["d16", "16 values"],
    ],
  },
  {
    title: "Skewed",
    shapes: [
      ["zipf1.1", "Zipf 1.1"],
      ["zipf2", "Zipf 2"],
      ["skew99", "99% one key"],
    ],
  },
  {
    title: "Presorted",
    shapes: [
      ["sorted", "sorted"],
      ["reverse", "reverse"],
      ["sorted99", "99% sorted"],
      ["runs", "sorted runs"],
    ],
  },
];
const TYPE_GROUPS: { title: string; keys: boolean; types: [string, string, string][] }[] = [
  {
    title: "Keys · against any sort",
    keys: true,
    types: [
      ["u32", "u32", "4-byte key"],
      ["u64", "u64", "8-byte key"],
      ["f64", "f64", "total order"],
    ],
  },
  {
    title: "Records · against stable sorts",
    keys: false,
    types: [
      ["row_u32", "{u32, u32}", "8-byte row"],
      ["row_u64", "{u64, u32}", "16-byte row"],
      ["record128", "128-byte record", "u64 key"],
      ["owned", "owned record", "{u64, String}"],
    ],
  },
];

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

type Tone = "tessera" | "radix" | "comparison";
const toneOf = (variant: string): Tone =>
  variant === "tessera" ? "tessera" : RADIX.has(variant) ? "radix" : "comparison";
// Drawing order: Tessera last, on top of the field, its filled default
// build above the hollow SIMD ring.
const DEPTH: Record<Tone, number> = { comparison: 0, radix: 1, tessera: 2 };

interface Mark {
  value: number;
  tone: Tone;
  dy?: number;
  hollow?: boolean;
}
interface Row {
  name: NameCell;
  swarm?: boolean;
  marks: Mark[];
  span?: [number, number];
  cells: string[];
  strong?: boolean;
}
interface Scale {
  low: number;
  high: number;
  ticks: { value: number; label: string }[];
  origin?: number;
  clamp: boolean;
}
interface Chart {
  title: string;
  guide: string;
  columns: string[];
  groups: { title: string; rows: Row[] }[];
  scale: Scale;
  marker?: { value: number; label: string };
  legend: (out: string[], t: Text, y: number) => void;
  axis: [string, string];
}

// A symmetric speed axis in doublings around slice::sort, wide enough for
// the data. Left of the centre the labels are fractions, so a tick and a
// value cell read the same way.
function speedScale(ratios: number[]): Scale {
  const reach = Math.max(2, ...ratios.map((r) => Math.min(RATIO_LIMIT, Math.max(r, 1 / r)) * 1.08));
  const span = 2 ** Math.ceil(Math.log2(reach));
  const steps = Math.log2(span);
  // Every doubling is labelled while there is room; wider axes label every other one.
  const every = steps <= 3 ? 1 : 2;
  const ticks = [{ value: 1, label: "slice::sort" }];
  for (let k = every; k <= steps; k += every) {
    ticks.push({ value: 2 ** k, label: `${2 ** k}×` }, { value: 2 ** -k, label: `1/${2 ** k}×` });
  }
  return { low: 1 / span, high: span, ticks, origin: 1, clamp: true };
}

// Blue and grey come from the pull-request chart; the orange passes the
// palette checks against both and both surfaces (CVD ΔE 11, normal ΔE 17).
const extraStyles = `<style>
  .tessera { fill: #4a80e6; } .radix { fill: #e8622f; } .comparison { fill: #8d8c86; }
  .edge-tessera { stroke: #4a80e6; } .edge-radix { stroke: #e8622f; } .edge-comparison { stroke: #8d8c86; }
</style>`;

function drawMark(out: string[], x: number, rowY: number, m: Mark, edge: 0 | -1 | 1) {
  const cy = rowY + (m.dy ?? 0);
  const cls = m.tone;
  if (edge) {
    const tip = x + edge * 6,
      back = x - edge * 3;
    out.push(
      `<path class="${cls} ring" d="M${tip.toFixed(1)} ${cy}L${back.toFixed(1)} ${cy - 6}L${back.toFixed(1)} ${cy + 6}Z" stroke-width="1.5" stroke-linejoin="round"/>`,
    );
    return;
  }
  if (m.hollow) {
    out.push(
      `<circle class="within edge-${m.tone}" cx="${x.toFixed(1)}" cy="${cy}" r="${m.tone === "tessera" ? 5.5 : 4}" stroke-width="${m.tone === "tessera" ? 2 : 1.5}"/>`,
    );
    return;
  }
  const r = m.tone === "tessera" ? 6.5 : 4;
  out.push(
    `<circle class="${cls} ring" cx="${x.toFixed(1)}" cy="${cy}" r="${r}" stroke-width="2"/>`,
  );
}

// Grey dots that would touch another dot move to a lane just above or below
// the row, so every sort stays visible; Tessera keeps the centre line.
function dodge(marks: Mark[], x: (v: number) => number) {
  const radius = (m: Mark) => (m.tone === "tessera" ? 6.5 : 4);
  const placed = marks.filter((m) => m.tone === "tessera").map((m) => ({ m, px: x(m.value) }));
  const clearance = (m: Mark, px: number, dy: number) =>
    Math.min(
      ...placed.map((p) => Math.hypot(p.px - px, (p.m.dy ?? 0) - dy) - radius(p.m) - radius(m)),
    );
  for (const m of [...marks]
    .filter((m) => m.tone !== "tessera")
    .sort((a, b) => a.value - b.value)) {
    const px = x(m.value);
    const lanes = [0, -7, 7];
    m.dy =
      lanes.find((dy) => clearance(m, px, dy) >= 1) ??
      lanes.reduce((best, dy) => (clearance(m, px, dy) > clearance(m, px, best) ? dy : best));
    placed.push({ m, px });
  }
}

// Header: the finding as the title, what is plotted under it, then a rule.
function drawTitle(out: string[], t: Text, width: number, spec: Chart) {
  t("ink stat", PAD, 40, spec.title);
  t("muted sub", PAD, 60, spec.guide);
  out.push(
    `<line class="rule" x1="${PAD}" x2="${width - PAD}" y1="${HEADER_RULE}" y2="${HEADER_RULE}" stroke-width="1"/>`,
  );
}

// A sort or input name, on one line when it has no second line.
function drawRowName(out: string[], t: Text, name: NameCell, cy: number) {
  if (name.sub) drawName(out, t, name, cy);
  else t(`ink${name.strong ? " strong" : ""}`, PAD, cy + 4, name.input);
}

function renderChart(spec: Chart) {
  const rows = spec.groups.flatMap((g) => g.rows);
  const nameW = nameWidth(rows.map((r) => r.name));
  const rowH = rows.some((r) => r.name.sub) ? ROW_H : ROW_H_SINGLE;
  // Value columns, right-aligned from the edge; caps headings run wide.
  const columnW = spec.columns.map((head, j) =>
    Math.max(textWidth(head, 11) * 1.3, ...rows.map((r) => textWidth(r.cells[j], 12) * 1.05)),
  );
  const columnR = columnW.map((_, j) =>
    columnW.slice(j + 1).reduce((right, w) => right - w - 20, W - PAD),
  );
  const plotL = PAD + Math.ceil(nameW) + 28;
  const plotR = columnR[0] - columnW[0] - 24;
  const room = 10;
  const { low, high } = spec.scale;
  const x = (v: number) =>
    plotL +
    room +
    (Math.log(Math.min(high, Math.max(low, v)) / low) / Math.log(high / low)) *
      (plotR - plotL - 2 * room);
  const edgeOf = (v: number): 0 | -1 | 1 =>
    spec.scale.clamp ? (v < low ? -1 : v > high ? 1 : 0) : 0;

  let y = PLOT_TOP;
  const layout = spec.groups.map((g, i) => {
    y += i ? 10 : 0;
    const titleY = y + 12;
    y += 22;
    const placed = g.rows.map((row) => {
      const cy = y + rowH / 2;
      y += rowH;
      return { row, cy };
    });
    return { title: g.title, titleY, placed };
  });
  const plotBottom = y + 4;
  const axisY = plotBottom + 18;
  const legendY = axisY + 34;
  const H = legendY + 18;

  const { out, t } = svgWriter();
  out.push(
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}" role="img" aria-labelledby="t d">`,
  );
  out.push(`<title id="t">${escapeXml(spec.title)}</title>`);
  out.push(
    `<desc id="d">${escapeXml(
      `${spec.guide}. ${spec.columns.join(", ")}: ${rows.map((r) => `${r.name.input} ${r.cells.join(", ")}`).join("; ")}`,
    )}</desc>`,
  );
  out.push(chartStyles, extraStyles);
  out.push(`<rect class="panel" x="0.5" y="0.5" width="${W - 1}" height="${H - 1}" rx="12"/>`);

  drawTitle(out, t, W, spec);

  for (const tick of spec.scale.ticks) {
    const gx = x(tick.value).toFixed(1);
    const origin = tick.value === spec.scale.origin;
    out.push(
      `<line class="${origin ? "axis" : "rule"}" x1="${gx}" x2="${gx}" y1="${PLOT_TOP}" y2="${plotBottom}" stroke-width="1"/>`,
    );
    t(
      origin ? "ink tiny" : "muted tiny",
      x(tick.value),
      axisY,
      tick.label,
      ' text-anchor="middle"',
    );
  }
  if (spec.marker) {
    const mx = x(spec.marker.value).toFixed(1);
    out.push(
      `<line class="axis" x1="${mx}" x2="${mx}" y1="${PLOT_TOP - 2}" y2="${plotBottom}" stroke-width="1" stroke-dasharray="3 3"/>`,
    );
    t("muted tiny", x(spec.marker.value), PLOT_TOP - 4, spec.marker.label, ' text-anchor="middle"');
  }
  // The axis ends say in words which side is which.
  const [left, right] = spec.axis;
  t("muted sub", plotL, axisY + 16, left);
  t("muted sub", plotR, axisY + 16, right, ' text-anchor="end"');

  for (const [j, head] of spec.columns.entries())
    t("caps muted", columnR[j], layout[0].titleY, head.toUpperCase(), ' text-anchor="end"');
  for (const g of layout) {
    t("caps muted", PAD, g.titleY, g.title.toUpperCase());
    for (const { row, cy } of g.placed) {
      drawRowName(out, t, row.name, cy);
      if (row.span) {
        out.push(
          `<line class="edge-${row.marks[0].tone}" x1="${x(row.span[0]).toFixed(1)}" x2="${x(row.span[1]).toFixed(1)}" y1="${cy}" y2="${cy}" stroke-width="2" stroke-linecap="round"/>`,
        );
      }
      if (row.swarm) dodge(row.marks, x);
      const depth = (m: Mark) => DEPTH[m.tone] + (m.hollow ? 0 : 0.5);
      for (const m of [...row.marks].sort((a, b) => depth(a) - depth(b)))
        drawMark(out, x(m.value), cy, m, edgeOf(m.value));
      // Tessera's cells are ink, its last cell bold; other sorts' are muted.
      for (const [j, cell] of row.cells.entries()) {
        const cls = !row.strong
          ? "val muted"
          : j === row.cells.length - 1
            ? "val ink strong"
            : "val ink";
        t(cls, columnR[j], cy + 4, cell, ' text-anchor="end"');
      }
    }
  }

  spec.legend(out, t, legendY);
  out.push("</svg>");
  return `<!-- SPDX-License-Identifier: BSD-2-Clause -->\n${out.join("\n")}\n`;
}

type Swatch = Tone | "simd" | "within" | "dash";
function legendRow(out: string[], t: Text, y: number, items: [Swatch[], string][]) {
  let lx = PAD;
  for (const [swatches, label] of items) {
    for (const s of swatches) {
      const cx = lx + 4;
      if (s === "within" || s === "simd") {
        const edge = s === "simd" ? " edge-tessera" : "";
        out.push(
          `<circle class="within${edge}" cx="${cx}" cy="${y - 4}" r="3.5" stroke-width="1.5"/>`,
        );
      } else if (s === "dash")
        out.push(
          `<line class="axis" x1="${cx}" x2="${cx}" y1="${y - 11}" y2="${y + 3}" stroke-width="1" stroke-dasharray="3 3"/>`,
        );
      else out.push(`<circle class="${s}" cx="${cx}" cy="${y - 4}" r="4"/>`);
      lx += 10;
    }
    t("muted sub", lx + 4, y, label);
    lx += 4 + textWidth(label, 11) + 18;
  }
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
  [["tessera"], "Tessera"],
  ...(simd ? [[["simd"], "Tessera, Portable SIMD"] as [Swatch[], string]] : []),
  [["radix"], "other radix sorts"],
  [["comparison"], "comparison sorts"],
];
const SPEED_AXIS: [string, string] = ["← slower", "faster →"];
const speedLegend = (simd: boolean) => (out: string[], t: Text, y: number) =>
  legendRow(out, t, y, kindLegend(simd));

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
      tone: "tessera" as Tone,
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
    svg: renderChart({
      title,
      guide: "Speed relative to slice::sort, geometric mean over 10 input patterns",
      columns: ["Speed"],
      groups: [
        { title: "Stable sorts", rows: complete.filter((e) => e.stable).map(row) },
        { title: "Unstable sorts", rows: complete.filter((e) => !e.stable).map(row) },
      ].filter((g) => g.rows.length),
      scale: speedScale(complete.map((e) => e.speeds[at])),
      axis: SPEED_AXIS,
      legend: speedLegend(complete.some((e) => e.simd)),
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
      ...(s.simd === null ? [] : [{ value: s.simd, tone: "tessera" as Tone, hollow: true }]),
      { value: s.tessera, tone: "tessera" },
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

// Each input pattern of u64 keys: every sort competes, since keys make
// stability unobservable.
function renderInputs(
  dataset: Dataset,
  summaries: Map<string, Summary>,
  name: string,
  n: number,
): Figure | null {
  const variants = dataset.manifests.get("default")!.variants;
  const plotted: [string, Standing][] = [];
  const nsPerKey: number[] = [];
  const groups = SHAPE_GROUPS.map((g) => ({
    title: g.title,
    rows: g.shapes.flatMap(([shape, label]) => {
      const id = caseId({ ty: "u64", distribution: shape, len: n });
      const s = standing(summaries, variants, [id], false);
      const ns = summaries.get(measurementId("default", id, "tessera"))?.ns;
      if (!s || ns === undefined) return [];
      plotted.push([label, s]);
      nsPerKey.push(ns);
      return [fieldRow({ input: label, sub: "", tag: "" }, s)];
    }),
  })).filter((g) => g.rows.length);
  if (plotted.length !== shapes.length) return null;
  const standings = plotted.map(([, s]) => s);
  const title = fieldTitle(standings, "input patterns");
  return {
    name,
    title,
    svg: renderChart({
      title,
      guide: `${formatLength(n)} u64 keys; each dot is one sort, placed by its speed relative to slice::sort`,
      columns: ["Speed", "Rank"],
      groups,
      scale: speedScale(standings.flatMap((s) => [s.tessera, ...s.speeds.map((v) => v.speed)])),
      axis: SPEED_AXIS,
      legend: speedLegend(standings.some((s) => s.simd !== null)),
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
    svg: renderChart({
      title,
      guide: `${formatLength(LARGE)} elements; speed relative to slice::sort, geometric mean over 10 input patterns`,
      columns: ["Speed", "Rank"],
      groups,
      scale: speedScale(standings.flatMap((s) => [s.tessera, ...s.speeds.map((v) => v.speed)])),
      axis: SPEED_AXIS,
      legend: speedLegend(standings.some((s) => s.simd !== null)),
    }),
    table: fieldTable(plotted),
  };
}

function renderMemory(dataset: Dataset): Figure | null {
  const n = LARGEST;
  const peaks = (variant: string) => {
    const found = shapes.map(
      (shape) =>
        dataset.memory.find(
          (m) =>
            m.build === "default" &&
            m.variant === variant &&
            m.case_id === caseId({ ty: "u64", distribution: shape, len: n }),
        )?.peak_extra_bytes,
    );
    return found.every((v): v is number => v !== undefined)
      ? { random: found[0], worst: Math.max(...found) }
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
      strong: tone === "tessera",
    };
  };
  const input = n * 8;
  const title = `Tessera sorts ${formatLength(n)} u64 keys in at most ${formatBytes(anchor.peak.worst)} of extra memory`;
  return {
    name: "memory",
    title,
    svg: renderChart({
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
      legend: (out, t, y) =>
        legendRow(out, t, y, [
          ...kindLegend(false),
          [["within"], "random input"],
          [["dash"], "input size"],
        ]),
    }),
    table: markdownTable(["Sort", "Random input", "Worst of 10 inputs"], plotted),
  };
}

export function renderFigures(dataset: Dataset, summaries: Map<string, Summary>): Figure[] {
  return [
    renderRanking(dataset, summaries),
    renderInputs(dataset, summaries, "inputs", LARGE),
    renderInputs(dataset, summaries, "short", SHORT),
    renderTypes(dataset, summaries),
    renderMemory(dataset),
  ].filter((f): f is Figure => f !== null);
}
