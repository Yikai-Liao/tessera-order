// SPDX-License-Identifier: BSD-2-Clause

import { createFigure } from "./figure.ts";
import { drawName, type NameCell, nameWidth, type Text } from "./layout.ts";
import { textWidth } from "./svg.ts";
import { theme } from "./theme.ts";

const PAD = theme.layout.pad;
const ROW_H = theme.layout.row;
const ROW_H_SINGLE = theme.layout.singleRow;
const HEADER_RULE = theme.layout.headerRule;
const PLOT_TOP = HEADER_RULE + 14;

export type Tone = "tessera" | "radix" | "comparison";
// Drawing order: Tessera last, on top of the field. Hollow rings are see-through
// and drawn above filled dots, so a ring overlapping a dot stays a whole circle.
const DEPTH: Record<Tone, number> = { comparison: 0, radix: 1, tessera: 2 };

export interface Mark {
  value: number;
  tone: Tone;
  dy?: number;
  hollow?: boolean;
}
export interface PlotRow {
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
export interface RowChart {
  id?: string;
  title: string;
  guide: string;
  columns: string[];
  groups: { title: string; rows: PlotRow[] }[];
  scale: Scale;
  marker?: { value: number; label: string };
  legend: [Swatch[], string][] | { note: string };
  axis: [string, string];
}

// Blue and grey come from the pull-request chart; the orange passes the
// palette checks against both and both surfaces (CVD ΔE 11, normal ΔE 17).
const extraStyles = `<style>
  .tessera { fill: ${theme.palette.blue}; } .radix { fill: ${theme.palette.orange}; } .comparison { fill: ${theme.palette.grey}; }
  .edge-tessera { stroke: ${theme.palette.blue}; } .edge-radix { stroke: ${theme.palette.orange}; } .edge-comparison { stroke: ${theme.palette.grey}; }
</style>`;

const hollowRadius = (m: Mark) => (m.tone === "tessera" ? 5.5 : 4);

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
    const r = hollowRadius(m);
    const width = m.tone === "tessera" ? 2 : 1.5;
    const circle = `cx="${x.toFixed(1)}" cy="${cy}" r="${r}" fill="none"`;
    out.push(
      `<circle class="ring" ${circle} stroke-width="${width + 2}"/>`,
      `<circle class="edge-${m.tone}" ${circle} stroke-width="${width}"/>`,
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
function drawTitle(out: string[], t: Text, width: number, spec: RowChart) {
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

/** Render a grouped ratio or memory chart using the shared visual grammar.
 *
 * Values use a positive logarithmic scale. Groups contain named rows, marks
 * and right-aligned value cells; layout, typography and drawing order belong
 * to the template. The input is not modified. Use a unique `id` for inline SVGs.
 */
export function renderRows(spec: RowChart) {
  if (
    !Number.isFinite(spec.scale.low) ||
    !Number.isFinite(spec.scale.high) ||
    !(spec.scale.low > 0 && spec.scale.high > spec.scale.low)
  )
    throw new Error("A row chart requires a positive, increasing scale");
  const groups = spec.groups
    .filter((group) => group.rows.length)
    .map((group) => ({
      ...group,
      rows: group.rows.map((row) => ({
        ...row,
        marks: row.marks.map((mark) => ({ ...mark })),
      })),
    }));
  const rows = groups.flatMap((group) => group.rows);
  if (!rows.length || !spec.columns.length)
    throw new Error("A row chart requires rows and value columns");
  for (const row of rows) {
    const values = [...row.marks.map((mark) => mark.value), ...(row.span ?? [])];
    if (
      row.cells.length !== spec.columns.length ||
      values.some((value) => !Number.isFinite(value) || value <= 0)
    )
      throw new Error("Rows require matching value cells and positive finite plot values");
  }
  const nameW = nameWidth(rows.map((r) => r.name));
  const rowH = rows.some((r) => r.name.sub) ? ROW_H : ROW_H_SINGLE;
  // Value columns, right-aligned from the edge; caps headings run wide.
  const columnW = spec.columns.map((head, j) =>
    Math.max(textWidth(head, 11) * 1.3, ...rows.map((r) => textWidth(r.cells[j], 12) * 1.05)),
  );
  const columnsWidth = columnW.reduce((sum, width) => sum + width, 0) + (columnW.length - 1) * 20;
  const W = Math.max(theme.layout.width, 2 * PAD + Math.ceil(nameW) + 28 + 160 + 24 + columnsWidth);
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
  const layout = groups.map((g, i) => {
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

  const { out, t, finish } = createFigure({
    width: W,
    height: H,
    title: spec.title,
    id: spec.id,
    styles: extraStyles,
    description: `${spec.guide}. ${spec.columns.join(", ")}: ${rows.map((row) => `${row.name.input} ${row.cells.join(", ")}`).join("; ")}`,
  });

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
        // The line stops at a ring's edge rather than showing through it.
        const inset = (value: number) => {
          const ring = row.marks.find((m) => m.hollow && m.value === value);
          return ring ? hollowRadius(ring) : 0;
        };
        const [lo, hi] = row.span;
        out.push(
          `<line class="edge-${row.marks[0].tone}" x1="${(x(lo) + inset(lo)).toFixed(1)}" x2="${(x(hi) - inset(hi)).toFixed(1)}" y1="${cy}" y2="${cy}" stroke-width="2" stroke-linecap="round"/>`,
        );
      }
      if (row.swarm) dodge(row.marks, x);
      const depth = (m: Mark) => DEPTH[m.tone] + (m.hollow ? 0.5 : 0);
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

  if (Array.isArray(spec.legend)) legendRow(out, t, legendY, spec.legend);
  else t("muted sub", PAD, legendY, spec.legend.note);
  return `<!-- SPDX-License-Identifier: BSD-2-Clause -->\n${finish()}\n`;
}

export type Swatch = Tone | "simd" | "within" | "dash";
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
