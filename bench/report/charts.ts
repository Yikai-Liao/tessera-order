// SPDX-License-Identifier: BSD-2-Clause

import {
  drawHeader,
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

// Every figure is a matrix in the visual language of the pull-request
// benchmark chart: a question and one headline number per column above a
// rule, named rows in titled groups, and one dot per cell on a scale shared
// by all columns, with the value in a right-aligned column. Blue and orange
// keep their meaning from that chart: blue is less time for Tessera, orange
// is more, and a hollow grey dot is within the A/A noise.

const PAD = 24;
const ROW_H = 38;
const HEADER_RULE = 76;
const PLOT_TOP = HEADER_RULE + 14;

// Losses on short presorted or skewed inputs reach 1/50 against SIMD sorts.
// A fixed ±8× range keeps 25 px per doubling in a column; marks beyond it sit
// on the edge as a triangle, and the value column keeps the exact ratio.
const RATIO_DOMAIN: [number, number] = [1 / 8, 8];

const NAMES: Record<string, string> = {
  tessera: "Tessera",
  "std-stable": "slice::sort",
  "std-unstable": "slice::sort_unstable",
};
const RADIX = new Set(["radsort", "voracious", "rdst", "ips2ra", "clausecker"]);
const SIMD = new Set(["vqsort", "x86simdsort"]);
const displayName = (variant: string) => NAMES[variant] ?? variant;
const family = (variant: string) =>
  variant === "tessera" || RADIX.has(variant) ? "radix" : SIMD.has(variant) ? "SIMD" : "comparison";

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
const TYPE_GROUPS: { title: string; types: [string, string, string][] }[] = [
  {
    title: "Keys · against any sort",
    types: [
      ["u32", "u32", "4-byte key"],
      ["u64", "u64", "8-byte key"],
      ["f64", "f64", "total order"],
    ],
  },
  {
    title: "Records · against stable sorts",
    types: [
      ["row_u32", "{u32, u32}", "8-byte row"],
      ["row_u64", "{u64, u32}", "16-byte row"],
      ["record128", "128-byte record", "u64 key"],
      ["owned", "owned record", "{u64, String}"],
    ],
  },
];

const formatRatio = (v: number) => `${v >= 100 ? v.toFixed(0) : Number(v.toPrecision(2))}×`;
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

// Tone of one comparison, from Tessera's side: an opponent that is slower
// beyond the A/A range is a Tessera win.
type Tone = "faster" | "slower" | "within" | "tessera" | "other";
const toneOf = (s: Summary): Tone =>
  s.verdict === "slower" ? "faster" : s.verdict === "faster" ? "slower" : "within";
const verdictText = (tone: Tone) =>
  tone === "faster" ? "Tessera faster" : tone === "slower" ? "Tessera slower" : "within noise";

interface Mark {
  value: number;
  tone: Tone;
  stem?: boolean;
  dy?: number;
  small?: boolean;
  hollow?: boolean;
}
interface Cell {
  marks: Mark[];
  span?: [number, number];
  tick?: number;
  text: string;
  strong?: boolean;
  note?: string;
}
interface Row {
  name: NameCell;
  cells: (Cell | null)[];
}
interface Column {
  hero: string;
  heroCls: string;
  name: string;
  sub: string;
  marker?: { value: number; label: string };
}
interface Matrix {
  title: string;
  heading: NameCell;
  columns: Column[];
  groups: { title: string; rows: Row[] }[];
  scale: {
    low: number;
    high: number;
    origin?: number;
    grid: number[];
    labels: number[];
    format: (v: number) => string;
    clamp?: boolean;
  };
  valueW: number;
  legend: (out: string[], t: Text, y: number, W: number) => void;
  desc: string;
}

// Classes the shared chart styles do not define: neutral marks and the ink
// stroke of a geometric-mean tick.
const extraStyles = `<style>
  .mark { fill: #8d8c86; } .stroke-ink { stroke: #0b0b0b; } .stroke-mark { stroke: #8d8c86; }
  @media (prefers-color-scheme: dark) { .stroke-ink { stroke: #e8e7e3; } }
</style>`;

function drawMark(out: string[], x: number, cy: number, m: Mark, edge: 0 | -1 | 1) {
  const y = cy + (m.dy ?? 0);
  const cls =
    m.tone === "tessera"
      ? "faster"
      : m.tone === "other"
        ? "mark"
        : m.tone === "within"
          ? "within"
          : m.tone;
  if (edge) {
    // An out-of-range value points off the scale.
    const tip = x + edge * 5,
      back = x - edge * 3;
    out.push(
      `<path class="${cls === "within" ? "within" : `${cls} ring`}" d="M${tip.toFixed(1)} ${y}L${back.toFixed(1)} ${y - 5}L${back.toFixed(1)} ${y + 5}Z" stroke-width="1.5" stroke-linejoin="round"/>`,
    );
    return;
  }
  if (m.tone === "within" || m.hollow) {
    out.push(
      `<circle class="within" cx="${x.toFixed(1)}" cy="${y}" r="${m.small ? 3 : 3.5}" stroke-width="1.5"${m.hollow && m.tone !== "within" ? ` style="stroke:${m.tone === "tessera" ? "#4a80e6" : "#8d8c86"}"` : ""}/>`,
    );
    return;
  }
  const r = m.small ? 3.5 : m.tone === "tessera" ? 5.5 : m.tone === "other" ? 3.5 : 4.5;
  out.push(
    `<circle class="${cls} ring" cx="${x.toFixed(1)}" cy="${y}" r="${r}" stroke-width="${m.small ? 1.5 : 2}"/>`,
  );
}

function renderMatrix(spec: Matrix) {
  const rows = spec.groups.flatMap((g) => g.rows);
  const nameW = nameWidth([...rows.map((r) => r.name), spec.heading]);
  const count = spec.columns.length;
  const W = 720 + Math.max(0, count - 2) * 180;
  const gap = 20;
  const plotL = PAD + Math.ceil(nameW) + 24;
  const colW = (W - PAD - plotL - gap * (count - 1)) / count;
  const cols = spec.columns.map((c, i) => {
    const left = plotL + i * (colW + gap);
    return { ...c, left, right: left + colW, plotR: left + colW - spec.valueW - 8 };
  });
  const room = 7;
  const { low, high } = spec.scale;
  const position = (col: (typeof cols)[number], v: number) => {
    const clamped = Math.min(high, Math.max(low, v));
    return (
      col.left +
      room +
      (Math.log(clamped / low) / Math.log(high / low)) * (col.plotR - col.left - 2 * room)
    );
  };
  const edgeOf = (v: number): 0 | -1 | 1 =>
    spec.scale.clamp ? (v < low ? -1 : v > high ? 1 : 0) : 0;

  let y = PLOT_TOP;
  const layout = spec.groups.map((g, i) => {
    y += i ? 10 : 0;
    const titleY = y + 12;
    y += 22;
    const placed = g.rows.map((row) => {
      const cy = y + ROW_H / 2;
      y += ROW_H;
      return { row, cy };
    });
    return { title: g.title, titleY, placed };
  });
  const plotBottom = y + 4;
  const H = plotBottom + 52;

  const { out, t } = svgWriter();
  out.push(
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}" role="img" aria-labelledby="t d">`,
  );
  out.push(`<title id="t">${escapeXml(spec.title)}</title>`);
  out.push(`<desc id="d">${escapeXml(spec.desc)}</desc>`);
  out.push(chartStyles, extraStyles);
  out.push(`<rect class="panel" x="0.5" y="0.5" width="${W - 1}" height="${H - 1}" rx="12"/>`);

  t("ink strong", PAD, 36, spec.heading.input);
  t("muted sub", PAD, 51, spec.heading.sub);
  for (const col of cols) drawHeader(t, col, col.hero, col.heroCls, col.name, col.sub);
  out.push(
    `<line class="rule" x1="${PAD}" x2="${W - PAD}" y1="${HEADER_RULE}" y2="${HEADER_RULE}" stroke-width="1"/>`,
  );

  for (const col of cols) {
    for (const v of spec.scale.grid) {
      const gx = position(col, v).toFixed(1);
      const origin = v === spec.scale.origin;
      out.push(
        `<line class="${origin ? "axis" : "rule"}" x1="${gx}" x2="${gx}" y1="${PLOT_TOP}" y2="${plotBottom}" stroke-width="1"/>`,
      );
    }
    for (const v of spec.scale.labels)
      t(
        "muted tiny",
        position(col, v),
        plotBottom + 16,
        spec.scale.format(v),
        ' text-anchor="middle"',
      );
    if (col.marker) {
      const mx = position(col, col.marker.value);
      out.push(
        `<line class="axis" x1="${mx.toFixed(1)}" x2="${mx.toFixed(1)}" y1="${PLOT_TOP - 2}" y2="${plotBottom}" stroke-width="1" stroke-dasharray="3 3"/>`,
      );
      t("muted tiny", mx, PLOT_TOP - 4, col.marker.label, ' text-anchor="middle"');
    }
  }

  for (const g of layout) {
    t("caps muted", PAD, g.titleY, g.title.toUpperCase());
    for (const { row, cy } of g.placed) {
      drawName(out, t, row.name, cy);
      row.cells.forEach((cell, i) => {
        const col = cols[i];
        if (!cell) {
          t("muted val", col.right, cy + 4, "—", ' text-anchor="end"');
          return;
        }
        const origin = spec.scale.origin;
        if (cell.span) {
          const [a, b] = cell.span.map((v) => position(col, v));
          const tessera = cell.marks.some((m) => m.tone === "tessera");
          out.push(
            `<line class="${tessera ? "stem-faster" : "stroke-mark"}" x1="${a.toFixed(1)}" x2="${b.toFixed(1)}" y1="${cy}" y2="${cy}" stroke-width="2" stroke-linecap="round"${tessera ? "" : ' stroke-opacity=".6"'}/>`,
          );
        }
        for (const m of cell.marks) {
          if (m.stem && origin !== undefined && m.tone !== "within") {
            const x0 = position(col, origin),
              x1 = position(col, m.value);
            out.push(
              `<line class="stem-${m.tone === "slower" ? "slower" : "faster"}" x1="${x0.toFixed(1)}" x2="${x1.toFixed(1)}" y1="${cy + (m.dy ?? 0)}" y2="${cy + (m.dy ?? 0)}" stroke-width="2" stroke-linecap="round"/>`,
            );
          }
        }
        if (cell.tick !== undefined) {
          const tx = position(col, cell.tick).toFixed(1);
          out.push(
            `<line class="stroke-ink" x1="${tx}" x2="${tx}" y1="${cy - 10}" y2="${cy + 10}" stroke-width="2" stroke-linecap="round"/>`,
          );
        }
        for (const m of cell.marks) drawMark(out, position(col, m.value), cy, m, edgeOf(m.value));
        const cls = cell.strong ? "val ink strong" : "val muted";
        if (cell.note) {
          t(cls, col.right, cy - 1, cell.text, ' text-anchor="end"');
          t("muted tiny", col.right, cy + 12, cell.note, ' text-anchor="end"');
        } else t(cls, col.right, cy + 4, cell.text, ' text-anchor="end"');
      });
    }
  }

  spec.legend(out, t, plotBottom + 38, W);
  out.push("</svg>");
  return `<!-- SPDX-License-Identifier: BSD-2-Clause -->\n${out.join("\n")}\n`;
}

// Legend pieces, laid out left to right from the panel's padding.
type Swatch = "faster" | "slower" | "within" | "mark" | "tick" | "dash" | "ring";
function legendRow(out: string[], t: Text, y: number, items: [Swatch[], string][]) {
  let x = PAD;
  for (const [swatches, label] of items) {
    for (const s of swatches) {
      const cx = x + 4;
      if (s === "within" || s === "ring")
        out.push(`<circle class="within" cx="${cx}" cy="${y - 4}" r="3.5" stroke-width="1.5"/>`);
      else if (s === "tick")
        out.push(
          `<line class="stroke-ink" x1="${cx}" x2="${cx}" y1="${y - 11}" y2="${y + 3}" stroke-width="2" stroke-linecap="round"/>`,
        );
      else if (s === "dash")
        out.push(
          `<line class="axis" x1="${cx}" x2="${cx}" y1="${y - 11}" y2="${y + 3}" stroke-width="1" stroke-dasharray="3 3"/>`,
        );
      else out.push(`<circle class="${s}" cx="${cx}" cy="${y - 4}" r="4"/>`);
      x += 10;
    }
    t("muted sub", x + 4, y, label);
    x += 4 + textWidth(label, 11) + 16;
  }
}
function directionCue(t: Text, y: number, W: number, left: string, right: string, colored = true) {
  t(colored ? "faster sub" : "muted sub", W - PAD, y, right, ' text-anchor="end"');
  t(
    colored ? "slower sub" : "muted sub",
    W - PAD - textWidth(right, 11) - 14,
    y,
    left,
    ' text-anchor="end"',
  );
}
const ratioScale = {
  low: RATIO_DOMAIN[0],
  high: RATIO_DOMAIN[1],
  origin: 1,
  grid: [1 / 8, 1 / 4, 1 / 2, 1, 2, 4, 8],
  labels: [1 / 8, 1, 8],
  format: (v: number) => (v < 1 ? "⅛×" : `${v}×`),
  clamp: true,
};

function opponents(summaries: Map<string, Summary>, id: string, eligible: (s: Summary) => boolean) {
  return [...summaries.values()].filter(
    (s) =>
      s.build === "default" && s.caseId === id && !s.variant.startsWith("tessera") && eligible(s),
  );
}
const fastest = (list: Summary[]) =>
  list.length ? list.reduce((a, b) => (b.ratio < a.ratio ? b : a)) : null;

// Speed of a variant relative to slice::sort in the same blocks, as a
// geometric mean over the ten input shapes.
function speedAgainstStd(
  summaries: Map<string, Summary>,
  variant: string,
  build: string,
  n: number,
) {
  const values = shapes.map((shape) => {
    const id = caseId({ ty: "u64", distribution: shape, len: n });
    const std = summaries.get(measurementId(build, id, "std-stable")),
      s = summaries.get(measurementId(build, id, variant));
    if (!std || !s) return null;
    return variant === "tessera" ? std.ratio : median(std.ratios.map((r, i) => r / s.ratios[i]));
  });
  return values.every((v): v is number => v !== null) ? geomean(values) : null;
}

function renderRanking(dataset: Dataset, summaries: Map<string, Summary>): Figure | null {
  const manifest = dataset.manifests.get("default")!;
  const entries = manifest.variants.map((v) => ({
    name: displayName(v.name),
    sub: family(v.name),
    stable: v.stable,
    tessera: v.name === "tessera",
    speeds: lengths.map((n) => speedAgainstStd(summaries, v.name, "default", n)),
  }));
  if (dataset.manifests.has("portable-simd"))
    entries.push({
      name: "Tessera, Portable SIMD",
      sub: "radix · SIMD build",
      stable: true,
      tessera: true,
      speeds: lengths.map((n) => speedAgainstStd(summaries, "tessera", "portable-simd", n)),
    });
  const complete = entries
    .filter((e) => e.speeds.every((v) => v !== null))
    .map((e) => ({ ...e, speeds: e.speeds as number[] }));
  const anchor = complete.find((e) => e.name === "Tessera" && e.tessera);
  if (!anchor) return null;
  const last = lengths.length - 1;
  complete.sort((a, b) => b.speeds[last] - a.speeds[last]);

  const row = (e: (typeof complete)[number]): Row => ({
    name: { input: e.name, sub: e.sub, tag: "", strong: e.tessera },
    cells: e.speeds.map((v) => ({
      marks: [{ value: v, tone: e.tessera ? "tessera" : "other", stem: e.tessera }],
      text: formatRatio(v),
      strong: e.tessera,
    })),
  });
  const groups = [
    { title: "Stable", rows: complete.filter((e) => e.stable).map(row) },
    { title: "Unstable", rows: complete.filter((e) => !e.stable).map(row) },
  ].filter((g) => g.rows.length);

  const columns: Column[] = lengths.map((n, j) => {
    const rank = 1 + complete.filter((e) => !e.tessera && e.speeds[j] > anchor.speeds[j]).length;
    const others = complete.filter((e) => !e.tessera).length;
    return {
      hero: formatRatio(anchor.speeds[j]),
      heroCls: anchor.speeds[j] >= 1 ? "faster" : "slower",
      name: formatLength(n),
      sub: rank === 1 ? `ahead of all ${others} others` : `${ordinal(rank)} of ${others + 1}`,
    };
  });
  const title = "Speed against slice::sort";
  const svg = renderMatrix({
    title,
    heading: { input: title, sub: "u64 keys · geometric mean of 10 inputs", tag: "", strong: true },
    columns,
    groups,
    scale: ratioScale,
    valueW: 40,
    legend: (out, t, y, W) => {
      legendRow(out, t, y, [
        [["faster"], "Tessera"],
        [["mark"], "other sorts"],
      ]);
      t("muted sub", W - PAD, y, "1× is slice::sort · faster →", ' text-anchor="end"');
    },
    desc: complete
      .map(
        (e) =>
          `${e.name}: ${e.speeds.map((v, j) => `${formatLength(lengths[j])} ${formatRatio(v)}`).join(", ")}`,
      )
      .join("; "),
  });
  return {
    name: "ranking",
    title,
    svg,
    table: markdownTable(
      ["Variant", "Kind", "Stability", ...lengths.map(formatLength)],
      complete.map((e) => [
        e.name,
        e.sub,
        e.stable ? "stable" : "unstable",
        ...e.speeds.map(formatRatio),
      ]),
    ),
  };
}

// One length of u64 keys, every input shape against the fastest radix,
// comparison and SIMD sort. Keys make stability unobservable, so every sort
// is eligible.
function renderInputs(summaries: Map<string, Summary>, n: number): Figure | null {
  const families = ["radix", "comparison", "SIMD"].filter((f) =>
    shapes.some(
      (shape) =>
        opponents(
          summaries,
          caseId({ ty: "u64", distribution: shape, len: n }),
          (s) => family(s.variant) === f,
        ).length,
    ),
  );
  const tessera = (shape: string) =>
    summaries.get(
      measurementId("default", caseId({ ty: "u64", distribution: shape, len: n }), "tessera"),
    );
  if (!families.length || !shapes.every(tessera)) return null;
  const best = (shape: string, f: string) =>
    fastest(
      opponents(
        summaries,
        caseId({ ty: "u64", distribution: shape, len: n }),
        (s) => family(s.variant) === f,
      ),
    );
  const plotted: string[][] = [];
  const groups = SHAPE_GROUPS.map((g) => ({
    title: g.title,
    rows: g.shapes.map(
      ([shape, label]): Row => ({
        name: { input: label, sub: `Tessera ${formatNs(tessera(shape)!.ns)} / key`, tag: "" },
        cells: families.map((f) => {
          const s = best(shape, f);
          if (!s) return null;
          const tone = toneOf(s);
          plotted.push([label, f, displayName(s.variant), formatRatio(s.ratio), verdictText(tone)]);
          return {
            marks: [{ value: s.ratio, tone, stem: true }],
            text: formatRatio(s.ratio),
            strong: tone !== "within",
            note: displayName(s.variant),
          };
        }),
      }),
    ),
  }));
  const columns: Column[] = families.map((f) => {
    const found = shapes.map((shape) => best(shape, f)).filter((s): s is Summary => s !== null);
    const wins = found.filter((s) => toneOf(s) === "faster").length,
      losses = found.filter((s) => toneOf(s) === "slower").length;
    const g = geomean(found.map((s) => s.ratio));
    return {
      hero: formatRatio(g),
      heroCls: g >= 1 ? "faster" : "slower",
      name: f === "radix" ? "Radix sorts" : f === "SIMD" ? "SIMD sorts" : "Comparison sorts",
      sub: `wins ${wins} · loses ${losses} of ${found.length}`,
    };
  });
  const short = n <= 1000;
  const title = short
    ? `Short arrays: ${formatLength(n)} u64 keys`
    : `Every input against the fastest of each kind, ${formatLength(n)} u64 keys`;
  return {
    name: short ? "short" : "inputs",
    title,
    svg: renderMatrix({
      title,
      heading: {
        input: `${formatLength(n)} u64 keys`,
        sub: "against the fastest of each kind",
        tag: "",
        strong: true,
      },
      columns,
      groups,
      scale: ratioScale,
      valueW: Math.max(44, ...plotted.map((r) => textWidth(r[2], 10))),
      legend: (out, t, y, W) => {
        legendRow(out, t, y, [
          [["faster", "slower"], "outside A/A noise"],
          [["within"], "within noise"],
        ]);
        directionCue(t, y, W, "← Tessera slower", "Tessera faster →");
      },
      desc: groups
        .flatMap((g) => g.rows)
        .map(
          (r) =>
            `${r.name.input}: ${r.cells.map((c, i) => `${families[i]} ${c ? `${c.text} ${c.note}` : "none"}`).join(", ")}`,
        )
        .join("; "),
    }),
    table: markdownTable(["Input", "Kind", "Fastest", "Its time / Tessera's", "Verdict"], plotted),
  };
}

// Spread placement for a strip of marks: each mark takes the first of three
// lanes where it clears the previous mark in that lane.
function dodge(marks: Mark[], px: (v: number) => number) {
  const lanes = [0, -6, 6];
  const lastX = lanes.map(() => -Infinity);
  for (const m of [...marks].sort((a, b) => a.value - b.value)) {
    const x = px(m.value);
    const lane = lanes.findIndex((_, i) => x - lastX[i] >= 7);
    const chosen = lane === -1 ? lastX.indexOf(Math.min(...lastX)) : lane;
    m.dy = lanes[chosen];
    lastX[chosen] = x;
  }
}

// Each element type and length against the one sort that comes closest over
// all ten inputs, so every dot in a row compares Tessera with the same sort a
// user could choose instead.
function renderTypes(summaries: Map<string, Summary>): Figure | null {
  const plotted: string[][] = [];
  const rowMeans = lengths.map(() => [] as number[]);
  // Lanes are assigned in scale space; 150 px is close to a column's plot width.
  const px = (v: number) =>
    (Math.log(Math.min(RATIO_DOMAIN[1], Math.max(RATIO_DOMAIN[0], v))) / Math.log(64)) * 150;
  const groups = TYPE_GROUPS.map((g) => ({
    title: g.title,
    rows: g.types
      .map(([ty, label, sub]): Row | null => {
        const keys = ["u32", "u64", "f64"].includes(ty);
        const cells = lengths.map((n, j): Cell | null => {
          const ids = shapes.map((shape) => caseId({ ty, distribution: shape, len: n }));
          const candidates = new Set(
            ids.flatMap((id) =>
              opponents(summaries, id, (s) => keys || s.stable).map((s) => s.variant),
            ),
          );
          const series = [...candidates]
            .map((variant) => ids.map((id) => summaries.get(measurementId("default", id, variant))))
            .filter((list): list is Summary[] => list.every((s) => s !== undefined))
            .map((list) => ({ list, mean: geomean(list.map((s) => s.ratio)) }));
          if (!series.length) return null;
          const { list, mean } = series.reduce((a, b) => (b.mean < a.mean ? b : a));
          rowMeans[j].push(mean);
          list.forEach((s, i) => {
            plotted.push([
              label,
              formatLength(n),
              shapes[i],
              displayName(s.variant),
              formatRatio(s.ratio),
              verdictText(toneOf(s)),
            ]);
          });
          const marks: Mark[] = list.map((s) => ({ value: s.ratio, tone: toneOf(s), small: true }));
          dodge(marks, px);
          return {
            marks,
            tick: mean,
            text: formatRatio(mean),
            strong: true,
            note: displayName(list[0].variant),
          };
        });
        return cells.some((c) => c) ? { name: { input: label, sub, tag: "" }, cells } : null;
      })
      .filter((r): r is Row => r !== null),
  })).filter((g) => g.rows.length);
  if (!groups.length) return null;
  const columns: Column[] = lengths.map((n, j) => {
    const means = rowMeans[j];
    return {
      hero: `${means.filter((m) => m > 1).length}/${means.length}`,
      heroCls: "ink",
      name: formatLength(n),
      sub: "types ahead",
    };
  });
  const title = "Every element type against its strongest alternative";
  return {
    name: "types",
    title,
    svg: renderMatrix({
      title,
      heading: {
        input: "Element types",
        sub: "against the strongest single sort",
        tag: "",
        strong: true,
      },
      columns,
      groups,
      scale: ratioScale,
      valueW: Math.max(36, ...plotted.map((r) => textWidth(r[3], 10))),
      legend: (out, t, y, W) => {
        legendRow(out, t, y, [
          [["faster", "slower", "within"], "one input each"],
          [["tick"], "geometric mean of 10"],
        ]);
        directionCue(t, y, W, "← Tessera slower", "Tessera faster →");
      },
      desc: groups
        .flatMap((g) => g.rows)
        .map(
          (r) =>
            `${r.name.input}: ${r.cells.map((c, j) => `${formatLength(lengths[j])} ${c ? `${c.text} against ${c.note}` : "not measured"}`).join(", ")}`,
        )
        .join("; "),
    }),
    table: markdownTable(
      ["Type", "Length", "Input", "Strongest alternative", "Its time / Tessera's", "Verdict"],
      plotted,
    ),
  };
}

function renderMemory(dataset: Dataset): Figure | null {
  const ns = lengths.slice(1);
  const peaks = (variant: string, n: number) => {
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
      ? { random: found[0], worst: Math.max(...found), best: Math.min(...found) }
      : null;
  };
  const manifest = dataset.manifests.get("default")!;
  const entries = manifest.variants
    .map((v) => ({ ...v, values: ns.map((n) => peaks(v.name, n)) }))
    .filter((e) => e.values.every((v) => v));
  const anchor = entries.find((e) => e.name === "tessera");
  if (!anchor) return null;
  const last = ns.length - 1;
  entries.sort((a, b) => a.values[last]!.worst - b.values[last]!.worst);
  const low = 100,
    high = 1e9;
  const plotted: string[][] = [];
  const row = (e: (typeof entries)[number]): Row => {
    const tessera = e.name === "tessera";
    return {
      name: { input: displayName(e.name), sub: family(e.name), tag: "", strong: tessera },
      cells: e.values.map((v, j) => {
        const { random, worst } = v!;
        plotted.push([
          displayName(e.name),
          formatLength(ns[j]),
          formatBytes(random),
          formatBytes(worst),
        ]);
        if (!worst) return { marks: [], text: "none" };
        const tone: Tone = tessera ? "tessera" : "other";
        return {
          marks: [
            ...(random !== worst ? [{ value: Math.max(low, random), tone, hollow: true }] : []),
            { value: worst, tone },
          ],
          span: random !== worst ? [Math.max(low, random), worst] : undefined,
          text: formatBytes(worst),
          strong: tessera,
        };
      }),
    };
  };
  const groups = [
    { title: "Stable", rows: entries.filter((e) => e.stable).map(row) },
    { title: "Unstable", rows: entries.filter((e) => !e.stable).map(row) },
  ].filter((g) => g.rows.length);
  const columns: Column[] = ns.map((n, j) => {
    const v = anchor.values[j]!;
    return {
      hero: formatBytes(v.worst),
      heroCls: "ink",
      name: formatLength(n),
      sub: `input ${formatBytes(n * 8)}`,
      marker: { value: n * 8, label: "input" },
    };
  });
  const decades = [100, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9];
  const title = "Extra heap memory";
  return {
    name: "memory",
    title,
    svg: renderMatrix({
      title,
      heading: { input: title, sub: "u64 keys · peak during one sort", tag: "", strong: true },
      columns,
      groups,
      scale: {
        low,
        high,
        grid: decades,
        labels: [1e3, 1e6, 1e9].filter((v) => v <= high),
        format: formatBytes,
      },
      valueW: 44,
      legend: (out, t, y, W) => {
        legendRow(out, t, y, [
          [["mark"], "worst of 10 inputs"],
          [["ring"], "random input"],
          [["dash"], "input size"],
        ]);
        t("muted sub", W - PAD, y, "log scale · ← less memory", ' text-anchor="end"');
      },
      desc: entries
        .map(
          (e) =>
            `${displayName(e.name)}: ${e.values.map((v, j) => `${formatLength(ns[j])} worst ${formatBytes(v!.worst)}`).join(", ")}`,
        )
        .join("; "),
    }),
    table: markdownTable(["Variant", "Length", "Random input", "Worst of 10 inputs"], plotted),
  };
}

export function renderFigures(dataset: Dataset, summaries: Map<string, Summary>): Figure[] {
  return [
    renderRanking(dataset, summaries),
    renderInputs(summaries, 1000000),
    renderInputs(summaries, 1000),
    renderTypes(summaries),
    renderMemory(dataset),
  ].filter((f): f is Figure => f !== null);
}
