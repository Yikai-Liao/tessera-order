// SPDX-License-Identifier: BSD-2-Clause

import { formatChange, formatPercent, isMajor, secondary, type ComparedRow, type ReportSummary } from "./report.ts";

function escapeXml(text: string) {
  const entities: Record<string, string> = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" };
  return text.replace(/[&<>"]/g, c => entities[c]);
}

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

// Approximate advances of Inter in em, enough to lay out a label column
// without a font engine.
function textWidth(text: string, px: number) {
  let em = 0;
  for (const c of text) {
    em += /[il.,:;'·| ]/.test(c) ? 0.28 : /[{}()[\]frtj]/.test(c) ? 0.36
      : /[mwMW%×]/.test(c) ? 0.86 : /[0-9]/.test(c) ? 0.62 : /[A-Z]/.test(c) ? 0.68 : 0.55;
  }
  return em * px;
}

// The chart of the alerted cases, as a standalone SVG for an image
// attachment. It follows the documentation figures: neutral ink, hairline
// rules, tabular figures, and brand blue (faster) against orange (slower).
// The pair passes the colour-vision checks on the light and dark surfaces,
// and the media query swaps only the neutrals, so one file serves both themes.
export function chartSvg(s: ReportSummary) {
  const groups = [
    { key: "regression", title: "SLOWER", cls: "slower" },
    { key: "improvement", title: "FASTER", cls: "faster" },
  ].map(g => ({ ...g, rows: s.rows.filter((r): r is ComparedRow => r.verdict === g.key) }))
    .filter(g => g.rows.length);
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
    y = first + g.rows.length * rowH + 10;
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
  out.push(`<style>
  .panel { fill: #fafaf8; stroke: #d8d7d2; } .ring { stroke: #fafaf8; }
  .ink { fill: #0b0b0b; } .muted { fill: #6b6a66; } .rule { stroke: #e6e5e0; } .axis { stroke: #8d8c86; }
  .tag { fill: none; stroke: #c9c8c2; }
  .slower { fill: #e8622f; } .faster { fill: #4a80e6; }
  .stem-slower { stroke: #e8622f; } .stem-faster { stroke: #4a80e6; }
  text { font-family: Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif; font-size: 13px; font-feature-settings: "tnum", "cv11"; }
  .val { font-size: 12px; } .sub { font-size: 11px; } .tiny { font-size: 10px; } .strong { font-weight: 600; }
  .hero { font-size: 28px; font-weight: 600; letter-spacing: -0.6px; font-feature-settings: "pnum"; }
  .stat { font-size: 18px; font-weight: 600; }
  .caps { font-size: 10.5px; font-weight: 600; letter-spacing: 1.1px; }
  @media (prefers-color-scheme: dark) {
    .panel { fill: #1e1e1d; stroke: #3a3a37; } .ring { stroke: #1e1e1d; }
    .ink { fill: #e8e7e3; } .muted { fill: #a3a29b; } .rule { stroke: #30302e; } .axis { stroke: #6f6e69; }
    .tag { stroke: #4a4a46; }
  }
</style>`);
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
  }
  out.push("</svg>");
  return out.join("\n");
}
