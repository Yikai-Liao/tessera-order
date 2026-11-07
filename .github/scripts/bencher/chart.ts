// SPDX-License-Identifier: BSD-2-Clause

import { formatChange, formatPercent, isMajor, secondary, type ComparedRow, type ReportSummary } from "./report.ts";
import { chartStyles, escapeXml, textWidth } from "./svg.ts";

// Each side of the axis ends at its largest change, so no width goes to
// empty range.
function sideReach(rows: ComparedRow[]) {
  return rows.length ? Math.max(...rows.map(r => Math.abs(r.change))) : 0;
}

// Tick step: the smallest round percentage whose ticks stay legible.
const STEPS = [0.05, 0.1, 0.2, 0.25, 0.5, 1, 2, 5];

function largeTickStep(minimum: number) {
  const magnitude = 10 ** Math.floor(Math.log10(minimum));
  const leading = minimum / magnitude;
  return (leading <= 1 ? 1 : leading <= 2 ? 2 : leading <= 5 ? 5 : 10) * magnitude;
}

// Faster cases below this change share one summary line instead of a row, so
// the chart grows with the notable changes rather than with the suite. It is
// a display choice: alerts still decide what is faster, the headline counts
// every alert, and the All Cases table lists each case. Slower cases are never
// folded. The PR history chart has its own rule (MIN_SWING in history.ts),
// because a regression fixed within the PR leaves no trace in this comparison.
export const FOLD_FASTER_BELOW = 0.05;

// The chart of the alerted cases, as a standalone SVG for an image
// attachment. It follows the documentation figures: neutral ink, hairline
// rules, tabular figures, and brand blue (faster) against orange (slower).
// The pair passes the colour-vision checks on the light and dark surfaces,
// and the media query swaps only the neutrals, so one file serves both themes.
export function chartSvg(s: ReportSummary) {
  // Folding every row would leave an axis with nothing on it, so small
  // speedups are drawn when nothing else is.
  const fold = s.rows.some(r => r.verdict === "regression" ||
    (r.verdict === "improvement" && -r.change >= FOLD_FASTER_BELOW));
  const groups = [
    { key: "regression", title: "SLOWER", cls: "slower" },
    { key: "improvement", title: "FASTER", cls: "faster" },
  ].map(g => {
    const alerted = s.rows.filter((r): r is ComparedRow => r.verdict === g.key);
    const shown = fold && g.key === "improvement" ? alerted.filter(r => -r.change >= FOLD_FASTER_BELOW) : alerted;
    return { ...g, rows: shown, folded: alerted.length - shown.length };
  }).filter(g => g.rows.length || g.folded);
  if (!groups.length) return null;
  const all = groups.flatMap(g => g.rows);
  const slower = all.filter(r => r.verdict === "regression");
  const faster = all.filter(r => r.verdict === "improvement");

  // Type scale: headline 28, names 13, values 12, secondary 11, titles 10.5.
  const W = 720;
  const pad = 24;
  const rowH = 38;
  const tagW = (r: ComparedRow) => (r.parts.extra ? textWidth(r.parts.extra, 10) + 12 : 0);
  const nameW = Math.max(...all.map(r => Math.max(
    textWidth(r.parts.input, 13) + (tagW(r) ? tagW(r) + 6 : 0),
    textWidth(secondary(r), 11))));
  const plotL = pad + Math.ceil(nameW) + 20;
  const plotR = W - pad;
  // One scale for both sides, each side only as long as its data, with room
  // beyond the outermost dot for its value.
  const room = 52;
  const rf = sideReach(faster);
  const rs = sideReach(slower);
  const leftRoom = faster.length ? room : 8;
  const rightRoom = slower.length ? room : 8;
  const k = (plotR - plotL - leftRoom - rightRoom) / (rf + rs);
  const zero = plotL + leftRoom + rf * k;
  const x = (r: ComparedRow) => zero + r.change * k;

  const heroBottom = 76;
  const plotTop = heroBottom + 14;
  let y = plotTop;
  const layout = groups.map(g => {
    const titleY = y + 12;
    const first = y + 22;
    y = first + g.rows.length * rowH + (g.folded ? 22 : 0) + 10;
    return { ...g, titleY, first };
  });
  const plotBottom = y - 6;
  const H = plotBottom + 44;

  const out: string[] = [];
  const t = (cls: string, xPos: number, yPos: number, text: string, extra = "") =>
    out.push(`<text class="${cls}" x="${xPos.toFixed(1)}" y="${yPos.toFixed(1)}"${extra}>${text}</text>`);
  out.push(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}" role="img" aria-labelledby="t d">`);
  out.push(`<title id="t">Benchmark changes: ${s.regressions} slower, ${s.improvements} faster</title>`);
  out.push(`<desc id="d">${escapeXml(all.map(r => `${r.label}: run time ${formatChange(r.change)}`).join("; "))}</desc>`);
  out.push(chartStyles);
  out.push(`<rect class="panel" x="0.5" y="0.5" width="${W - 1}" height="${H - 1}" rx="12"/>`);

  // Headline: the overall ratio, then the counts.
  if (s.geomean != null) {
    const g = s.geomean;
    const dir = g < 1 ? "faster" : g > 1 ? "slower" : "ink";
    const pct = formatChange(g - 1);
    t(`hero ${dir}`, pad, 50, pct);
    const lx = pad + textWidth(pct, 28) + 14;
    t("ink", lx, 36, "run time overall");
    t("muted sub", lx, 51, `geometric mean of ${s.compared} cases`);
  }
  const stats = ([[s.improvements, "faster", "faster"], [s.regressions, "slower", "slower"], [s.missing, "new", "ink"]] satisfies [number, string, string][])
    .filter(([n]) => n);
  let sx = plotR;
  for (const [n, word, cls] of [...stats].reverse()) {
    t("muted sub", sx, 50, word, ' text-anchor="end"');
    sx -= textWidth(word, 11) + 5;
    t(`stat ${cls}`, sx, 50, String(n), ' text-anchor="end"');
    sx -= textWidth(String(n), 18) + 22;
  }
  out.push(`<line class="rule" x1="${pad}" x2="${plotR}" y1="${heroBottom}" y2="${heroBottom}" stroke-width="1"/>`);

  // Rules at a round step on each side, at least 60 px apart so the grid
  // stays quiet.
  const step = STEPS.find(v => v * k >= 60) ?? largeTickStep(60 / k);
  for (const [side, reach] of [[-1, rf], [1, rs]]) {
    for (let v = step; v <= reach + 1e-9; v += step) {
      const gx = zero + side * v * k;
      out.push(`<line class="rule" x1="${gx.toFixed(1)}" x2="${gx.toFixed(1)}" y1="${plotTop}" y2="${plotBottom}" stroke-width="1"/>`);
      t("muted tiny", gx, plotBottom + 16, formatPercent(side * v), ' text-anchor="middle"');
    }
  }
  out.push(`<line class="axis" x1="${zero.toFixed(1)}" x2="${zero.toFixed(1)}" y1="${plotTop}" y2="${plotBottom}" stroke-width="1"/>`);
  t("muted tiny", zero, plotBottom + 16, "0", ' text-anchor="middle"');
  t("faster sub", zero - 8, plotBottom + 33, "← less time", ' text-anchor="end"');
  t("slower sub", zero + 8, plotBottom + 33, "more time →");

  for (const g of layout) {
    t(`caps ${g.cls}`, pad, g.titleY, g.title);
    g.rows.forEach((r, i) => {
      const cy = g.first + i * rowH + rowH / 2;
      const strong = isMajor(r) ? " strong" : "";
      t(`ink${strong}`, pad, cy - 2, escapeXml(r.parts.input));
      if (r.parts.extra) {
        const tx = pad + textWidth(r.parts.input, 13) + 6;
        out.push(`<rect class="tag" x="${tx.toFixed(1)}" y="${cy - 13}" width="${tagW(r).toFixed(1)}" height="14" rx="7" stroke-width="1"/>`);
        t("muted tiny", tx + tagW(r) / 2, cy - 3, escapeXml(r.parts.extra), ' text-anchor="middle"');
      }
      t("muted sub", pad, cy + 13, escapeXml(secondary(r)));
      const dx = x(r);
      out.push(`<line class="stem-${g.cls}" x1="${zero.toFixed(1)}" x2="${dx.toFixed(1)}" y1="${cy}" y2="${cy}" stroke-width="2" stroke-linecap="round"/>`);
      out.push(`<circle class="${g.cls} ring" cx="${dx.toFixed(1)}" cy="${cy}" r="${isMajor(r) ? 5.5 : 4}" stroke-width="2"/>`);
      const left = r.verdict === "improvement";
      t(`val ink${strong}`, dx + (left ? -10 : 10), cy + 4, formatChange(r.change),
        ` text-anchor="${left ? "end" : "start"}"`);
    });
    if (g.folded) {
      t("muted sub", pad, g.first + g.rows.length * rowH + 14,
        `${g.folded} more faster, each by less than ${Math.round(FOLD_FASTER_BELOW * 100)}%`);
    }
  }
  out.push("</svg>");
  return out.join("\n");
}
