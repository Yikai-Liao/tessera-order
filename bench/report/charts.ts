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
// answers, each row is one named thing with one mark, and the axis and the
// values are words ("3.5× faster", "4× slower") rather than ratios. Blue and
// orange keep their meaning from the pull-request benchmark chart: blue is
// less time for Tessera, orange is more, and a hollow grey dot is within the
// A/A noise.

const PAD = 24;
const W = 720;
const ROW_H = 38;
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
// glidesort-sqrt receives Tessera's measured peak as its buffer, so its
// memory matches Tessera's by construction.
const family = (variant: string) =>
  variant === "glidesort-sqrt"
    ? "glidesort with Tessera's memory"
    : variant === "tessera" || RADIX.has(variant)
      ? "radix sort"
      : SIMD.has(variant)
        ? "SIMD sort"
        : "comparison sort";

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
// A ratio is the other sort's time over Tessera's; above 1 Tessera is faster.
const formatSpeed = (r: number) => (r >= 1 ? `${times(r)} faster` : `${times(1 / r)} slower`);
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

// Tone of one comparison from Tessera's side: an opponent that is slower
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
  hollow?: boolean;
}
interface Row {
  name: NameCell;
  marks: Mark[];
  span?: [number, number];
  text: string;
  strong?: boolean;
  note?: string;
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
  hero: { value: string; cls: string; label: string; sub: string };
  groups: { title: string; rows: Row[] }[];
  scale: Scale;
  marker?: { value: number; label: string };
  legend: (out: string[], t: Text, y: number) => void;
  axis: [string, string];
}

// A symmetric speed axis in doublings around "same", wide enough for the
// data and labelled in words on both sides.
function speedScale(ratios: number[]): Scale {
  const reach = Math.max(2, ...ratios.map((r) => Math.min(RATIO_LIMIT, Math.max(r, 1 / r)) * 1.08));
  const span = 2 ** Math.ceil(Math.log2(reach));
  const steps = Math.log2(span);
  // Every doubling is labelled while there is room; wider axes label every other one.
  const every = steps <= 3 ? 1 : 2;
  const ticks = [{ value: 1, label: "same" }];
  for (let k = every; k <= steps; k += every) {
    ticks.push({ value: 2 ** k, label: `${2 ** k}×` }, { value: 2 ** -k, label: `${2 ** k}×` });
  }
  return { low: 1 / span, high: span, ticks, origin: 1, clamp: true };
}

const extraStyles = `<style>
  .mark { fill: #8d8c86; } .stroke-mark { stroke: #8d8c86; }
</style>`;

function drawMark(out: string[], x: number, cy: number, m: Mark, edge: 0 | -1 | 1) {
  const cls = m.tone === "tessera" ? "faster" : m.tone === "other" ? "mark" : m.tone;
  if (edge) {
    const tip = x + edge * 6,
      back = x - edge * 3;
    out.push(
      `<path class="${cls === "within" ? "within" : `${cls} ring`}" d="M${tip.toFixed(1)} ${cy}L${back.toFixed(1)} ${cy - 6}L${back.toFixed(1)} ${cy + 6}Z" stroke-width="1.5" stroke-linejoin="round"/>`,
    );
    return;
  }
  if (m.tone === "within" || m.hollow) {
    const stroke = m.hollow && m.tone === "tessera" ? ' style="stroke:#4a80e6"' : "";
    out.push(
      `<circle class="within" cx="${x.toFixed(1)}" cy="${cy}" r="4" stroke-width="1.5"${stroke}/>`,
    );
    return;
  }
  const r = m.tone === "tessera" ? 6 : m.tone === "other" ? 4 : 5;
  out.push(
    `<circle class="${cls} ring" cx="${x.toFixed(1)}" cy="${cy}" r="${r}" stroke-width="2"/>`,
  );
}

function renderChart(spec: Chart) {
  const rows = spec.groups.flatMap((g) => g.rows);
  const nameW = nameWidth(rows.map((r) => r.name));
  const valueW = Math.max(
    ...rows.map((r) => Math.max(textWidth(r.text, 12) * 1.05, r.note ? textWidth(r.note, 10) : 0)),
  );
  const plotL = PAD + Math.ceil(nameW) + 28;
  const plotR = W - PAD - Math.ceil(valueW) - 24;
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
      const cy = y + ROW_H / 2;
      y += ROW_H;
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
      `${spec.hero.value} ${spec.hero.label}, ${spec.hero.sub}. ${rows.map((r) => `${r.name.input}: ${r.text}${r.note ? ` (${r.note})` : ""}`).join("; ")}`,
    )}</desc>`,
  );
  out.push(chartStyles, extraStyles);
  out.push(`<rect class="panel" x="0.5" y="0.5" width="${W - 1}" height="${H - 1}" rx="12"/>`);

  // Header: the question on the left, its answer on the right.
  t("ink strong", PAD, 36, spec.title);
  t("muted sub", PAD, 54, spec.guide);
  const labelW = Math.max(textWidth(spec.hero.label, 13) * 1.05, textWidth(spec.hero.sub, 11));
  const lx = W - PAD - labelW;
  t(`hero ${spec.hero.cls}`, lx - 12, 50, spec.hero.value, ' text-anchor="end"');
  t("ink strong", lx, 36, spec.hero.label);
  t("muted sub", lx, 51, spec.hero.sub);
  out.push(
    `<line class="rule" x1="${PAD}" x2="${W - PAD}" y1="${HEADER_RULE}" y2="${HEADER_RULE}" stroke-width="1"/>`,
  );

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
  // The axis says in words which side is which.
  const [left, right] = spec.axis;
  const tinted = spec.scale.origin !== undefined;
  t(tinted ? "slower sub" : "muted sub", plotL, axisY + 16, left);
  t(tinted ? "faster sub" : "muted sub", plotR, axisY + 16, right, ' text-anchor="end"');

  for (const g of layout) {
    t("caps muted", PAD, g.titleY, g.title.toUpperCase());
    for (const { row, cy } of g.placed) {
      drawName(out, t, row.name, cy);
      if (row.span) {
        const tessera = row.marks.some((m) => m.tone === "tessera");
        out.push(
          `<line class="${tessera ? "stem-faster" : "stroke-mark"}" x1="${x(row.span[0]).toFixed(1)}" x2="${x(row.span[1]).toFixed(1)}" y1="${cy}" y2="${cy}" stroke-width="2" stroke-linecap="round"/>`,
        );
      }
      for (const m of row.marks) {
        if (!m.stem || spec.scale.origin === undefined || m.tone === "within") continue;
        out.push(
          `<line class="stem-${m.tone === "slower" ? "slower" : "faster"}" x1="${x(spec.scale.origin).toFixed(1)}" x2="${x(m.value).toFixed(1)}" y1="${cy}" y2="${cy}" stroke-width="2" stroke-linecap="round"/>`,
        );
      }
      for (const m of row.marks) drawMark(out, x(m.value), cy, m, edgeOf(m.value));
      const cls = row.strong ? "val ink strong" : "val muted";
      if (row.note) {
        t(cls, W - PAD, cy - 1, row.text, ' text-anchor="end"');
        t("muted tiny", W - PAD, cy + 12, row.note, ' text-anchor="end"');
      } else t(cls, W - PAD, cy + 4, row.text, ' text-anchor="end"');
    }
  }

  spec.legend(out, t, legendY);
  out.push("</svg>");
  return `<!-- SPDX-License-Identifier: BSD-2-Clause -->\n${out.join("\n")}\n`;
}

type Swatch = "faster" | "slower" | "within" | "mark" | "dash";
function legendRow(out: string[], t: Text, y: number, items: [Swatch[], string][]) {
  let lx = PAD;
  for (const [swatches, label] of items) {
    for (const s of swatches) {
      const cx = lx + 4;
      if (s === "within")
        out.push(`<circle class="within" cx="${cx}" cy="${y - 4}" r="3.5" stroke-width="1.5"/>`);
      else if (s === "dash")
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
const verdictLegend = (out: string[], t: Text, y: number) =>
  legendRow(out, t, y, [
    [["faster"], "Tessera faster"],
    [["slower"], "Tessera slower"],
    [["within"], "difference within run-to-run noise"],
  ]);

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
  const entries = dataset.manifests.get("default")!.variants.map((v) => ({
    name: displayName(v.name),
    sub: family(v.name),
    stable: v.stable,
    tessera: v.name === "tessera",
    speeds: lengths.map((n) => speedAgainstStd(summaries, v.name, "default", n)),
  }));
  if (dataset.manifests.has("portable-simd"))
    entries.push({
      name: "Tessera, Portable SIMD",
      sub: "radix sort · SIMD build",
      stable: true,
      tessera: true,
      speeds: lengths.map((n) => speedAgainstStd(summaries, "tessera", "portable-simd", n)),
    });
  const at = lengths.indexOf(LARGE);
  const complete = entries
    .filter((e) => e.speeds.every((v) => v !== null))
    .map((e) => ({ ...e, speeds: e.speeds as number[] }))
    .sort((a, b) => b.speeds[at] - a.speeds[at]);
  const anchor = complete.find((e) => e.name === "Tessera");
  if (!anchor) return null;
  const others = complete.filter((e) => !e.tessera);
  const rank = 1 + others.filter((e) => e.speeds[at] > anchor.speeds[at]).length;
  const row = (e: (typeof complete)[number]): Row => ({
    name: { input: e.name, sub: e.sub, tag: "", strong: e.tessera },
    marks: [{ value: e.speeds[at], tone: e.tessera ? "tessera" : "other", stem: e.tessera }],
    text: e.name === "slice::sort" ? "baseline" : formatSpeed(e.speeds[at]),
    strong: e.tessera,
  });
  const title = `Sorting ${formatLength(LARGE)} u64 keys`;
  return {
    name: "ranking",
    title,
    svg: renderChart({
      title,
      guide: "Speed of each sort against slice::sort, averaged over 10 input patterns",
      hero: {
        value: times(anchor.speeds[at]),
        cls: anchor.speeds[at] >= 1 ? "faster" : "slower",
        label: "faster than slice::sort",
        sub:
          rank === 1
            ? `fastest of ${others.length + 1} sorts`
            : `${ordinal(rank)} of ${others.length + 1} sorts`,
      },
      groups: [
        { title: "Stable sorts", rows: complete.filter((e) => e.stable).map(row) },
        { title: "Unstable sorts", rows: complete.filter((e) => !e.stable).map(row) },
      ].filter((g) => g.rows.length),
      scale: speedScale(complete.map((e) => e.speeds[at])),
      axis: ["← slower than slice::sort", "faster than slice::sort →"],
      legend: (out, t, y) =>
        legendRow(out, t, y, [
          [["faster"], "Tessera"],
          [["mark"], "other sorts"],
        ]),
    }),
    table: markdownTable(
      ["Sort", "Kind", "Stability", ...lengths.map((n) => `${formatLength(n)} keys`)],
      complete.map((e) => [
        e.name,
        e.sub,
        e.stable ? "stable" : "unstable",
        ...e.speeds.map((v) => `${times(v)} slice::sort`),
      ]),
    ),
  };
}

// Each input pattern of u64 keys against the fastest eligible sort for that
// case, naming it. Keys make stability unobservable, so every sort competes.
function renderInputs(
  summaries: Map<string, Summary>,
  options: {
    name: string;
    n: number;
    title: string;
    guide: string;
    rivals: string;
    eligible: (s: Summary) => boolean;
  },
): Figure | null {
  const id = (shape: string) => caseId({ ty: "u64", distribution: shape, len: options.n });
  const tessera = (shape: string) => summaries.get(measurementId("default", id(shape), "tessera"));
  const best = (shape: string) => fastest(opponents(summaries, id(shape), options.eligible));
  if (!shapes.every((shape) => tessera(shape) && best(shape))) return null;
  const plotted: string[][] = [];
  const found: Summary[] = [];
  const groups = SHAPE_GROUPS.map((g) => ({
    title: g.title,
    rows: g.shapes.map(([shape, label]): Row => {
      const s = best(shape)!;
      const tone = toneOf(s);
      found.push(s);
      plotted.push([
        label,
        displayName(s.variant),
        formatSpeed(s.ratio),
        verdictText(tone),
        formatNs(tessera(shape)!.ns),
      ]);
      return {
        name: { input: label, sub: `Tessera ${formatNs(tessera(shape)!.ns)} per key`, tag: "" },
        marks: [{ value: s.ratio, tone, stem: true }],
        text: tone === "within" ? "even" : formatSpeed(s.ratio),
        strong: tone !== "within",
        note: `vs ${displayName(s.variant)}`,
      };
    }),
  }));
  const wins = found.filter((s) => toneOf(s) === "faster").length,
    losses = found.filter((s) => toneOf(s) === "slower").length;
  return {
    name: options.name,
    title: options.title,
    svg: renderChart({
      title: options.title,
      guide: options.guide,
      hero: {
        value: `${wins} of ${found.length}`,
        cls: wins >= losses ? "faster" : "slower",
        label: `inputs faster than ${options.rivals}`,
        sub: `slower on ${losses} · even on ${found.length - wins - losses}`,
      },
      groups,
      scale: speedScale(found.map((s) => s.ratio)),
      axis: ["← Tessera slower", "Tessera faster →"],
      legend: verdictLegend,
    }),
    table: markdownTable(
      ["Input", "Fastest rival", "Tessera", "Verdict", "Tessera time per key"],
      plotted,
    ),
  };
}

// Each element type against the one sort that comes closest over all ten
// inputs, so the comparison is with a sort a user could choose instead.
function renderTypes(summaries: Map<string, Summary>): Figure | null {
  const plotted: string[][] = [];
  const means: number[] = [];
  const tones: Tone[] = [];
  const groups = TYPE_GROUPS.map((g) => ({
    title: g.title,
    rows: g.types
      .map(([ty, label, sub]): Row | null => {
        const ids = shapes.map((shape) => caseId({ ty, distribution: shape, len: LARGE }));
        const candidates = new Set(
          ids.flatMap((id) =>
            opponents(summaries, id, (s) => g.keys || s.stable).map((s) => s.variant),
          ),
        );
        const series = [...candidates]
          .map((variant) => ids.map((id) => summaries.get(measurementId("default", id, variant))))
          .filter((list): list is Summary[] => list.every((s) => s !== undefined))
          .map((list) => ({ list, mean: geomean(list.map((s) => s.ratio)) }));
        if (!series.length) return null;
        const { list, mean } = series.reduce((a, b) => (b.mean < a.mean ? b : a));
        // A mean inside the widest A/A range of its ten cases is a tie.
        const tone: Tone =
          mean > Math.max(...list.map((s) => s.aaHigh))
            ? "faster"
            : mean < Math.min(...list.map((s) => s.aaLow))
              ? "slower"
              : "within";
        tones.push(tone);
        means.push(mean);
        const wins = list.filter((s) => toneOf(s) === "faster").length;
        plotted.push([
          label,
          displayName(list[0].variant),
          formatSpeed(mean),
          `${wins} of ${list.length}`,
        ]);
        return {
          name: { input: label, sub, tag: "" },
          marks: [{ value: mean, tone, stem: true }],
          text: tone === "within" ? "even" : formatSpeed(mean),
          strong: tone !== "within",
          note: `vs ${displayName(list[0].variant)}`,
        };
      })
      .filter((r): r is Row => r !== null),
  })).filter((g) => g.rows.length);
  if (!groups.length) return null;
  const ahead = tones.filter((t) => t === "faster").length,
    behind = tones.filter((t) => t === "slower").length;
  const title = `Every element type, ${formatLength(LARGE)} elements`;
  return {
    name: "types",
    title,
    svg: renderChart({
      title,
      guide: "Tessera against the strongest rival for each type, over 10 inputs",
      hero: {
        value: `${ahead} of ${means.length}`,
        cls: ahead >= behind ? "faster" : "slower",
        label: "types where Tessera leads",
        sub: `behind on ${behind} · even on ${means.length - ahead - behind}`,
      },
      groups,
      scale: speedScale(means),
      axis: ["← Tessera slower", "Tessera faster →"],
      legend: verdictLegend,
    }),
    table: markdownTable(["Type", "Strongest rival", "Tessera", "Inputs Tessera wins"], plotted),
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
    const tessera = e.name === "tessera";
    const { random, worst } = e.peak;
    plotted.push([displayName(e.name), formatBytes(random), formatBytes(worst)]);
    const tone: Tone = tessera ? "tessera" : "other";
    if (!worst)
      return {
        name: { input: displayName(e.name), sub: family(e.name), tag: "" },
        marks: [],
        text: "none",
      };
    const spread = random !== worst;
    return {
      name: { input: displayName(e.name), sub: family(e.name), tag: "", strong: tessera },
      marks: [
        ...(spread ? [{ value: Math.max(low, random), tone, hollow: true }] : []),
        { value: worst, tone },
      ],
      span: spread ? [Math.max(low, random), worst] : undefined,
      text: formatBytes(worst),
      strong: tessera,
      note: spread ? `random ${formatBytes(random)}` : undefined,
    };
  };
  const title = `Extra memory for ${formatLength(n)} u64 keys`;
  const input = n * 8;
  return {
    name: "memory",
    title,
    svg: renderChart({
      title,
      guide: `Heap allocated during one sort, beyond the ${formatBytes(input)} input`,
      hero: {
        value: formatBytes(anchor.peak.worst),
        cls: "ink",
        label: "Tessera at most",
        sub: `${formatBytes(anchor.peak.random)} on random input`,
      },
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
          [["mark"], "worst of 10 inputs"],
          [["within"], "random input"],
          [["dash"], `input size, ${formatBytes(input)}`],
        ]),
    }),
    table: markdownTable(["Sort", "Random input", "Worst of 10 inputs"], plotted),
  };
}

export function renderFigures(dataset: Dataset, summaries: Map<string, Summary>): Figure[] {
  const large = formatLength(LARGE),
    short = formatLength(SHORT);
  return [
    renderRanking(dataset, summaries),
    renderInputs(summaries, {
      name: "inputs",
      n: LARGE,
      title: `Each input pattern, ${large} u64 keys`,
      guide: "Tessera against the fastest other sort on that input",
      rivals: "every other sort",
      eligible: () => true,
    }),
    renderInputs(summaries, {
      name: "radix",
      n: LARGE,
      title: `Against other radix sorts, ${large} u64 keys`,
      guide: "Tessera against the fastest other radix sort on that input",
      rivals: "every radix sort",
      eligible: (s) => RADIX.has(s.variant),
    }),
    renderInputs(summaries, {
      name: "short",
      n: SHORT,
      title: `Short arrays, ${short} u64 keys`,
      guide: "Tessera against the fastest other sort on that input",
      rivals: "every other sort",
      eligible: () => true,
    }),
    renderTypes(summaries),
    renderMemory(dataset),
  ].filter((f): f is Figure => f !== null);
}
