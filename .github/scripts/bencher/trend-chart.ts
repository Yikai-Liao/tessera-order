// SPDX-License-Identifier: BSD-2-Clause

import { MIN_SWING, type TrendSummary } from "./history.ts";
import { caseParts, formatChange, formatTime, MAJOR, short } from "./report.ts";
import { chartStyles, escapeXml, textWidth } from "./svg.ts";

interface TrendRow {
  input: string;
  sub: string;
  extra: string;
  details: string;
  // Change against main, one per PR run.
  changes: number[];
  // Overall carries its change at both ends in the name style; a case carries
  // measured times at its ends and its change in the right column.
  aggregate: boolean;
  first: string;
  last: string;
  major: boolean;
}

function side(change: number) {
  return change < 0 ? "faster" : change > 0 ? "slower" : null;
}

// The PR history as a standalone SVG, drawn with the comparison chart's
// marks (chart.ts). Each row's dashed grey axis is main; its line is blue
// while the PR takes less time than main and orange while it takes more, and
// the end dot and the right column repeat the comparison's change. Colour
// therefore changes only where a line crosses main, not with rerun noise.
export function trendSvg(s: TrendSummary) {
  const latest = s.runs.length - 1;
  const overall: TrendRow = {
    input: "Overall", sub: `geometric mean, ${s.sharedCases} cases`, extra: "",
    details: `Geometric mean of ${s.sharedCases} cases measured on main and in every run`,
    changes: s.overall, aggregate: true, first: formatChange(s.overall[0]),
    last: formatChange(s.overall[latest]), major: true,
  };
  const caseRow = (series: TrendSummary["focus"][number]): TrendRow => {
    const p = caseParts(series.name);
    return {
      input: p.input, sub: [p.type, p.n].filter(Boolean).join(" · "), extra: p.extra,
      details: `${series.label}: main ${formatTime(series.baseValue)}, latest ${formatTime(series.lastValue)}`,
      changes: series.changes, aggregate: false, first: formatTime(series.firstValue),
      last: formatTime(series.lastValue), major: Math.abs(series.changes[latest]) >= MAJOR,
    };
  };
  // Cases keep history.ts's order, largest swing first; a line that rises
  // and one that falls need no group title to tell them apart.
  const rows = [overall, ...s.focus.map(caseRow)];

  // Type scale and spacing follow chart.ts; rows are taller to hold a line.
  const W = 720;
  const pad = 24;
  const rowH = 46;
  const tagW = (r: TrendRow) => (r.extra ? textWidth(r.extra, 10) + 12 : 0);
  const nameW = Math.max(110, ...rows.map(r => Math.max(
    textWidth(r.input, 13) + (tagW(r) ? tagW(r) + 6 : 0), textWidth(r.sub, 11))));
  const plotL = pad + Math.ceil(nameW) + 20;
  const cases = rows.filter(r => !r.aggregate);
  const changeW = Math.ceil(Math.max(0, ...cases.map(r => textWidth(formatChange(r.changes[latest]), 12)))) + 20;
  // Room beside each end of the line for its value, as beside the dots in chart.ts.
  const valueW = (r: TrendRow, v: string) => textWidth(v, r.aggregate ? 13 : 11);
  const lineL = plotL + Math.ceil(Math.max(...rows.map(r => valueW(r, r.first)))) + 12;
  const lineR = W - pad - changeW - Math.ceil(Math.max(...rows.map(r => valueW(r, r.last)))) - 14;
  const x = (i: number) => latest ? lineL + i * (lineR - lineL) / latest : (lineL + lineR) / 2;
  // A row's vertical scale spans its own range and main, but never less than
  // MIN_SWING, so a gap below the selection threshold does not fill the row
  // and look as large as one selected for its size.
  const bandH = 24;
  const yScale = (changes: number[], center: number) => {
    const low = Math.min(0, ...changes);
    const high = Math.max(0, ...changes);
    const span = Math.max(high - low, MIN_SWING);
    const mid = (high + low) / 2;
    return (v: number) => center - (v - mid) / span * bandH;
  };

  const heroBottom = 76;
  const plotTop = heroBottom + 10;
  // A hairline sets the cases off from Overall.
  const ruleY = plotTop + rowH + 6;
  const rowTop = (i: number) => plotTop + i * rowH + (i ? 12 : 0);
  const plotBottom = rowTop(rows.length) + 4;
  const H = plotBottom + 44;

  const out: string[] = [];
  const t = (cls: string, xPos: number, yPos: number, text: string, extra = "") =>
    out.push(`<text class="${cls}" x="${xPos.toFixed(1)}" y="${yPos.toFixed(1)}"${extra}>${escapeXml(text)}</text>`);
  // The headline is the movement within the PR; the gap to main is the
  // comparison chart's headline and the Overall row's end.
  const progress = (1 + s.overall[latest]) / (1 + s.overall[0]) - 1;
  out.push(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}" role="img" aria-labelledby="t d">`);
  out.push(`<title id="t">PR benchmark history: ${escapeXml(formatChange(progress))} run time over ${s.runs.length} runs</title>`);
  out.push(`<desc id="d">${escapeXml([
    `Change against main ${short(s.base)} on ${s.testbed}, runs ${short(s.runs[0].hash)} to ${short(s.runs[latest].hash)}`,
    ...rows.map(r => `${r.details}, ${r.changes.map(formatChange).join(", ")}`),
  ].join("; "))}</desc>`);
  out.push(chartStyles);
  out.push(`<style>
  .hit { fill: transparent; } .main { stroke-dasharray: 3 3; } .on-main { fill: #8d8c86; }
  @media (prefers-color-scheme: dark) { .on-main { fill: #6f6e69; } }
</style>`);
  out.push(`<rect class="panel" x="0.5" y="0.5" width="${W - 1}" height="${H - 1}" rx="12"/>`);

  const pct = formatChange(progress);
  t(`hero ${side(progress) ?? "ink"}`, pad, 50, pct);
  const lx = pad + textWidth(pct, 28) + 14;
  t("ink", lx, 36, "run time during this PR");
  t("muted sub", lx, 51, "first to latest run, geometric mean");
  t("muted sub", W - pad, 50, "runs", ' text-anchor="end"');
  t("stat ink", W - pad - textWidth("runs", 11) - 5, 50, String(s.runs.length), ' text-anchor="end"');
  out.push(`<line class="rule" x1="${pad}" x2="${W - pad}" y1="${heroBottom}" y2="${heroBottom}" stroke-width="1"/>`);

  // Hairlines at the first and latest run carry the only two commit labels.
  for (const i of latest ? [0, latest] : [0]) {
    out.push(`<line class="rule" x1="${x(i).toFixed(1)}" x2="${x(i).toFixed(1)}" y1="${plotTop}" y2="${plotBottom}" stroke-width="1"/>`);
    t("muted tiny", x(i), plotBottom + 16, short(s.runs[i].hash), ' text-anchor="middle"');
  }

  if (rows.length > 1) out.push(`<line class="rule" x1="${pad}" x2="${W - pad}" y1="${ruleY}" y2="${ruleY}" stroke-width="1"/>`);
  rows.forEach((r, i) => {
    const cy = rowTop(i) + rowH / 2;
    const strong = r.major ? " strong" : "";
    out.push(`<g><title>${escapeXml(r.details)}</title>`);
    t(`ink${strong}`, pad, cy - 2, r.input);
    if (r.extra) {
      const tx = pad + textWidth(r.input, 13) + 6;
      out.push(`<rect class="tag" x="${tx.toFixed(1)}" y="${cy - 13}" width="${tagW(r).toFixed(1)}" height="14" rx="7" stroke-width="1"/>`);
      t("muted tiny", tx + tagW(r) / 2, cy - 3, r.extra, ' text-anchor="middle"');
    }
    t("muted sub", pad, cy + 13, r.sub);

    const ly = yScale(r.changes, cy + 3);
    const zero = ly(0);
    out.push(`<line class="axis main" x1="${lineL}" x2="${lineR.toFixed(1)}" y1="${zero.toFixed(1)}" y2="${zero.toFixed(1)}" stroke-width="1"/>`);
    // Split each segment where it crosses main so each piece has one colour.
    const piece = (x0: number, v0: number, x1: number, v1: number) => {
      const cls = side(v0 + v1);
      out.push(`<line class="${cls ? `stem-${cls}` : "axis"}" x1="${x0.toFixed(1)}" y1="${ly(v0).toFixed(1)}" x2="${x1.toFixed(1)}" y2="${ly(v1).toFixed(1)}" stroke-width="2" stroke-linecap="round"/>`);
    };
    r.changes.slice(1).forEach((v1, k) => {
      const v0 = r.changes[k];
      if (v0 * v1 < 0) {
        const xc = x(k) + (x(k + 1) - x(k)) * v0 / (v0 - v1);
        piece(x(k), v0, xc, 0);
        piece(xc, 0, x(k + 1), v1);
      } else piece(x(k), v0, x(k + 1), v1);
    });
    // Only the two labelled runs get a visible dot; every run between
    // them is a vertex of the line and keeps an invisible hover target.
    s.runs.forEach((run, k) => {
      const v = r.changes[k];
      const end = k === latest;
      const mark = end || k === 0 ? `${side(v) ?? "on-main"} ring` : "hit";
      const radius = end ? (r.major ? 5.5 : 4) : k === 0 ? 2.5 : 5;
      const url = `https://bencher.dev/perf/${encodeURIComponent(s.project)}/reports/${encodeURIComponent(run.uuid)}`;
      out.push(`<a href="${url}"><title>${escapeXml(`${short(run.hash)} · ${run.time} · ${formatChange(v)} against main`)}</title>`);
      out.push(`<circle class="${mark}" cx="${x(k).toFixed(1)}" cy="${ly(v).toFixed(1)}" r="${radius}" stroke-width="${end ? 2 : 1.5}"/></a>`);
    });
    const endCls = r.aggregate ? "ink strong" : "muted sub";
    t(endCls, x(0) - 10, ly(r.changes[0]) + 4, r.first, ' text-anchor="end"');
    t(endCls, x(latest) + 12, ly(r.changes[latest]) + 4, r.last);
    if (!r.aggregate) t(`val ink${strong}`, W - pad, cy + 4, formatChange(r.changes[latest]), ' text-anchor="end"');
    out.push("</g>");
  });
  const legendY = plotBottom + 33;
  out.push(`<line class="axis main" x1="${pad}" x2="${pad + 14}" y1="${legendY - 4}" y2="${legendY - 4}" stroke-width="1"/>`);
  const mainLabel = `main ${short(s.base)}`;
  t("muted sub", pad + 20, legendY, mainLabel);
  const arrowsX = pad + 20 + textWidth(mainLabel, 11) + 18;
  t("faster sub", arrowsX, legendY, "↓ less time");
  t("slower sub", arrowsX + textWidth("↓ less time", 11) + 14, legendY, "↑ more time");
  t("muted tiny", W - pad, legendY, "each row on its own scale", ' text-anchor="end"');
  out.push("</svg>");
  return out.join("\n");
}
