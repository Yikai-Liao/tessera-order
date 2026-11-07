// SPDX-License-Identifier: BSD-2-Clause

import { changedRows, isFaster, modeName, type CaseRow, type Configuration } from "./matrix.ts";
import { formatChange, formatPercent, isMajor, MAJOR, type Row } from "./report.ts";
import { chartStyles, escapeXml, textWidth } from "./svg.ts";

// Faster cases below this change share one summary line instead of a row, so
// the chart grows with the notable changes rather than with the suite. It is
// a display choice: alerts still decide what is faster, the headline counts
// every alert, and the All Cases table lists each case. Slower cases are never
// folded. The PR history chart has its own rule (MIN_SWING in history.ts),
// because a regression fixed within the PR leaves no trace in this comparison.
export const FOLD_FASTER_BELOW = 0.05;

// Tick step: the smallest round percentage whose ticks stay legible.
const STEPS = [0.05, 0.1, 0.2, 0.25, 0.5, 1, 2, 5];

function largeTickStep(minimum: number) {
  const magnitude = 10 ** Math.floor(Math.log10(minimum));
  const leading = minimum / magnitude;
  return (leading <= 1 ? 1 : leading <= 2 ? 2 : leading <= 5 ? 5 : 10) * magnitude;
}

export function tickStep(k: number, minPx: number) {
  return STEPS.find(v => v * k >= minPx) ?? largeTickStep(minPx / k);
}

export const PAD = 24;
// Each value sits in a fixed right-aligned column of its cell, so values
// line up down a configuration and read as a table.
export const VALUE_W = 52;

// The case-name column and one column per configuration. Two configurations
// keep the 720 px width of a comment image; each further one adds a column
// rather than another chart, so the cases are named once however many builds
// are measured.
export function matrixLayout(count: number, nameW: number) {
  const W = 720 + Math.max(0, count - 2) * 180;
  const gap = 20;
  const plotL = PAD + Math.ceil(nameW) + 24;
  const colW = (W - PAD - plotL - gap * (count - 1)) / count;
  const cols = Array.from({ length: count }, (_, i) => {
    const left = plotL + i * (colW + gap);
    return { left, right: left + colW, plotR: left + colW - VALUE_W - 6 };
  });
  return { W, plotL, cols };
}

export interface NameCell {
  input: string;
  tag: string;
  sub: string;
  strong?: boolean;
}

export const tagWidth = (tag: string) => (tag ? textWidth(tag, 10) + 12 : 0);

export function nameWidth(names: NameCell[]) {
  // Bold text runs about 5% wider than the regular advances in textWidth.
  return Math.max(110, ...names.map(n => Math.max(textWidth(n.input, 13) * (n.strong ? 1.05 : 1) + (n.tag ? tagWidth(n.tag) + 6 : 0), textWidth(n.sub, 11))));
}

// A case's name, an optional pill for a non-default seed, and its type and
// length. The mode is the group title above it, not a pill.
export function nameCell(row: CaseRow, strong: boolean): NameCell {
  const { parts } = row;
  return {
    input: parts.input, strong,
    tag: parts.seed && parts.seed !== "s1" ? `seed ${parts.seed.slice(1)}` : "",
    sub: [parts.type, parts.n].filter(Boolean).join(" · "),
  };
}

export type Text = (cls: string, x: number, y: number, text: string, extra?: string) => void;

export function svgWriter() {
  const out: string[] = [];
  const t: Text = (cls, x, y, text, extra = "") =>
    out.push(`<text class="${cls}" x="${x.toFixed(1)}" y="${y.toFixed(1)}"${extra}>${escapeXml(text)}</text>`);
  return { out, t };
}

export function drawName(out: string[], t: Text, name: NameCell, cy: number) {
  t(`ink${name.strong ? " strong" : ""}`, PAD, cy - 2, name.input);
  if (name.tag) {
    const tx = PAD + textWidth(name.input, 13) + 6;
    out.push(`<rect class="tag" x="${tx.toFixed(1)}" y="${cy - 13}" width="${tagWidth(name.tag).toFixed(1)}" height="14" rx="7" stroke-width="1"/>`);
    t("muted tiny", tx + tagWidth(name.tag) / 2, cy - 3, name.tag, ' text-anchor="middle"');
  }
  t("muted sub", PAD, cy + 13, name.sub);
}

// The column header: a configuration's headline number, its name and a short
// line under it.
export function drawHeader(t: Text, col: { left: number; right: number }, hero: string, heroCls: string, name: string, sub: string, subCls = "muted sub") {
  t(`hero ${heroCls}`, col.left, 50, hero);
  const lx = col.left + textWidth(hero, 28) + 12;
  t("ink strong", lx, 36, name);
  t(subCls, lx, 51, sub);
}

function counts(summary: Configuration["summary"]) {
  if (!summary) return "";
  const parts = [];
  if (summary.regressions) parts.push(`${summary.regressions} slower`);
  if (summary.improvements) parts.push(`${summary.improvements} faster`);
  if (!parts.length) parts.push(summary.compared ? "no alerts" : "no baseline");
  return parts.join(" · ");
}

// The cases alerted in any configuration, as a dot plot with one column per
// configuration on one shared scale. Every column is compared with its own
// configuration's main baseline. A coloured dot on a stem is an alert, blue
// for less time and orange for more; a hollow grey dot is the same case
// within its limits in that configuration, so a change confined to one build
// is visible at a glance. The pair passes the colour-vision checks on the
// light and dark surfaces, and the media query swaps only the neutrals, so
// one file serves both themes.
export function chartSvg(configurations: Configuration[]) {
  const changed = changedRows(configurations);
  if (!changed.length) return null;
  // Folding every faster row would leave an axis with nothing on it, so small
  // speedups are drawn when nothing else is.
  const small = (row: CaseRow) => isFaster(row) &&
    row.cells.every(cell => cell?.verdict !== "improvement" || -cell.change! < FOLD_FASTER_BELOW);
  const fold = changed.some(row => !small(row));
  const shown = fold ? changed.filter(row => !small(row)) : changed;
  const folded = changed.length - shown.length;

  const majorRow = (row: CaseRow) => row.cells.some(cell => cell && cell.verdict !== "within" && cell.verdict !== "missing" && isMajor(cell));
  const names = shown.map(row => nameCell(row, majorRow(row)));
  // The header's two lines share the name column, so they count toward its width.
  const title: NameCell = { input: "Run time against main", tag: "", sub: "each build, own baseline", strong: true };
  const { W, cols } = matrixLayout(configurations.length, nameWidth([...names, title]));

  // One scale for every column, each side only as long as the data.
  const values = shown.flatMap(row => row.cells.filter((c): c is Row => c?.change != null).map(c => c.change!));
  const rf = Math.max(0, ...values.map(v => -v));
  const rs = Math.max(0, ...values);
  const span = Math.max(rf + rs, 1e-9);
  const room = 8;
  const k = (cols[0].plotR - cols[0].left - 2 * room) / span;
  const zeroOffset = room + rf * k;

  // Rows grouped by mode, each group under its title.
  const rowH = 38;
  const heroBottom = 76;
  const plotTop = heroBottom + 14;
  let y = plotTop;
  const groups: { title: string; titleY: number; rows: { row: CaseRow; name: NameCell; cy: number }[] }[] = [];
  shown.forEach((row, i) => {
    const title = modeName(row.parts.mode).toUpperCase();
    if (groups.at(-1)?.title !== title) {
      y += groups.length ? 10 : 0;
      groups.push({ title, titleY: y + 12, rows: [] });
      y += 22;
    }
    groups.at(-1)!.rows.push({ row, name: names[i], cy: y + rowH / 2 });
    y += rowH;
  });
  if (folded) y += 22;
  const plotBottom = y + 4;
  const H = plotBottom + 52;

  const { out, t } = svgWriter();
  out.push(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}" role="img" aria-labelledby="t d">`);
  out.push(`<title id="t">Benchmark changes against main: ${escapeXml(configurations.map(c => `${c.name} ${counts(c.summary) || "not measured"}`).join("; "))}</title>`);
  out.push(`<desc id="d">${escapeXml(shown.map(row => `${row.label}: ` + configurations.map((c, i) =>
    `${c.name} ${formatChange(row.cells[i]?.change ?? null)}${row.cells[i] && row.cells[i]!.verdict !== "within" ? ` ${row.cells[i]!.verdict}` : ""}`).join(", ")).join("; "))}</desc>`);
  out.push(chartStyles);
  out.push(`<rect class="panel" x="0.5" y="0.5" width="${W - 1}" height="${H - 1}" rx="12"/>`);

  // Header: the question on the left, each configuration's answer above its column.
  t("ink strong", PAD, 36, title.input);
  t("muted sub", PAD, 51, title.sub);
  configurations.forEach((c, i) => {
    const s = c.summary;
    if (!s) return drawHeader(t, cols[i], "—", "muted", c.name, "measurement failed", "slower sub");
    const g = s.geomean;
    drawHeader(t, cols[i], g == null ? "—" : formatChange(g - 1), g == null ? "muted" : g < 1 ? "faster" : g > 1 ? "slower" : "ink",
      c.name, counts(s));
  });
  out.push(`<line class="rule" x1="${PAD}" x2="${W - PAD}" y1="${heroBottom}" y2="${heroBottom}" stroke-width="1"/>`);

  // Each column: quiet grid, its zero axis and tick labels.
  const step = tickStep(k, 44);
  for (const col of cols) {
    const zero = col.left + zeroOffset;
    for (const [side, reach] of [[-1, rf], [1, rs]]) {
      for (let v = step; v <= reach + 1e-9; v += step) {
        const gx = zero + side * v * k;
        out.push(`<line class="rule" x1="${gx.toFixed(1)}" x2="${gx.toFixed(1)}" y1="${plotTop}" y2="${plotBottom}" stroke-width="1"/>`);
        t("muted tiny", gx, plotBottom + 16, formatPercent(side * v), ' text-anchor="middle"');
      }
    }
    out.push(`<line class="axis" x1="${zero.toFixed(1)}" x2="${zero.toFixed(1)}" y1="${plotTop}" y2="${plotBottom}" stroke-width="1"/>`);
    t("muted tiny", zero, plotBottom + 16, "0", ' text-anchor="middle"');
  }

  for (const g of groups) {
    t("caps muted", PAD, g.titleY, g.title);
    for (const { row, name, cy } of g.rows) {
      drawName(out, t, name, cy);
      row.cells.forEach((cell, i) => {
        const col = cols[i];
        if (!cell) return;
        if (cell.change == null) {
          t("muted sub", col.right, cy + 4, "new", ' text-anchor="end"');
          return;
        }
        const zero = col.left + zeroOffset;
        const dx = zero + cell.change * k;
        const strong = isMajor(cell) && cell.verdict !== "within" ? " strong" : "";
        if (cell.verdict === "within") {
          out.push(`<circle class="within" cx="${dx.toFixed(1)}" cy="${cy}" r="3.5" stroke-width="1.5"/>`);
          t("val muted", col.right, cy + 4, formatChange(cell.change), ' text-anchor="end"');
        } else {
          const cls = cell.verdict === "regression" ? "slower" : "faster";
          out.push(`<line class="stem-${cls}" x1="${zero.toFixed(1)}" x2="${dx.toFixed(1)}" y1="${cy}" y2="${cy}" stroke-width="2" stroke-linecap="round"/>`);
          out.push(`<circle class="${cls} ring" cx="${dx.toFixed(1)}" cy="${cy}" r="${strong ? 5.5 : 4}" stroke-width="2"/>`);
          t("val ink strong", col.right, cy + 4, formatChange(cell.change), ' text-anchor="end"');
        }
      });
    }
  }
  if (folded) {
    t("muted sub", PAD, plotBottom - 10, `${folded} more faster, each by less than ${Math.round(FOLD_FASTER_BELOW * 100)}%`);
  }

  // Legend: what the marks mean, then which way is better.
  const ly = plotBottom + 38;
  out.push(`<circle class="slower" cx="${PAD + 4}" cy="${ly - 4}" r="4"/>`);
  out.push(`<circle class="faster" cx="${PAD + 14}" cy="${ly - 4}" r="4"/>`);
  t("muted sub", PAD + 24, ly, "alert");
  const wx = PAD + 24 + textWidth("alert", 11) + 16;
  out.push(`<circle class="within" cx="${wx + 4}" cy="${ly - 4}" r="3.5" stroke-width="1.5"/>`);
  t("muted sub", wx + 14, ly, "within limits");
  const bx = wx + 14 + textWidth("within limits", 11) + 16;
  out.push(`<circle class="ink" cx="${bx + 5.5}" cy="${ly - 4}" r="5.5"/>`);
  t("muted sub", bx + 16, ly, `${MAJOR * 100}% or more`);
  t("slower sub", W - PAD, ly, "more time →", ' text-anchor="end"');
  t("faster sub", W - PAD - textWidth("more time →", 11) - 14, ly, "← less time", ' text-anchor="end"');
  out.push("</svg>");
  return out.join("\n");
}
