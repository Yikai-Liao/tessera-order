// SPDX-License-Identifier: BSD-2-Clause

import {
  drawHeader,
  drawName,
  matrixLayout,
  type NameCell,
  nameWidth,
  PAD,
  svgWriter,
} from "./chart.ts";
import { MIN_SWING, type TrendSeries, type TrendSummary } from "./history.ts";
import { caseParts, formatChange, MAJOR, short } from "./report.ts";
import { chartStyles, escapeXml } from "./svg.ts";

export interface ConfigurationTrend {
  name: string;
  trend: TrendSummary | null;
}

function side(change: number) {
  return change < 0 ? "faster" : change > 0 ? "slower" : null;
}

const swing = (changes: number[]) => (1 + Math.max(...changes)) / (1 + Math.min(...changes)) - 1;

// The PR history, drawn with the comparison chart's columns (chart.ts): one
// column per configuration, one row for each configuration's overall change
// and one per case that history.ts selected in any configuration. Each
// cell's dashed grey axis is the comparison base, `main` unless a stacked
// PR names its parent; its line is blue while the PR takes less time than
// the base and orange while it takes more, so colour changes only where a
// line crosses the base, not with rerun noise. A row's cells share one
// vertical scale, so the same swing has the same height in every build.
export function trendSvg(configurations: ConfigurationTrend[], base = "main") {
  const measured = configurations.filter((c) => c.trend);
  if (!measured.length) return null;
  const byName = configurations.map(
    (c) => new Map((c.trend?.series ?? []).map((series) => [series.name, series])),
  );
  // A case selected in any configuration gets a row in all of them; rows run
  // from the largest swing in any configuration down.
  const selected = [
    ...new Set(configurations.flatMap((c) => c.trend?.focus.map((series) => series.name) ?? [])),
  ];
  const largest = (name: string) =>
    Math.max(
      ...byName
        .map((m) => m.get(name))
        .filter((s): s is TrendSeries => !!s)
        .map((s) => swing(s.changes)),
    );
  selected.sort((a, b) => largest(b) - largest(a) || a.localeCompare(b));

  interface TrendRow {
    name: NameCell;
    cells: (number[] | undefined)[];
    aggregate: boolean;
    label: string;
  }
  const overall: TrendRow = {
    name: { input: "Overall", tag: "", sub: "geometric mean", strong: true },
    cells: configurations.map((c) => c.trend?.overall),
    aggregate: true,
    label: "Overall",
  };
  const rows: TrendRow[] = [
    overall,
    ...selected.map((name) => {
      const p = caseParts(name);
      const latest = byName.map((m) => m.get(name)?.changes.at(-1) ?? 0);
      return {
        name: {
          input: p.input,
          sub: [p.type, p.n].filter(Boolean).join(" · "),
          tag: [
            p.seed && p.seed !== "s1" ? `seed ${p.seed.slice(1)}` : "",
            p.mode !== "api" ? p.mode : "",
          ]
            .filter(Boolean)
            .join(", "),
          strong: latest.some((v) => Math.abs(v) >= MAJOR),
        },
        cells: byName.map((m) => m.get(name)?.changes),
        aggregate: false,
        label: name,
      };
    }),
  ];

  const title: NameCell = {
    input: "Run time during this PR",
    tag: "",
    sub: "first to latest run",
    strong: true,
  };
  const { W, cols } = matrixLayout(
    configurations.length,
    nameWidth([...rows.map((r) => r.name), title]),
  );
  const rowH = 46;
  const bandH = 24;
  const heroBottom = 76;
  const plotTop = heroBottom + 10;
  // A hairline sets the cases off from Overall.
  const rowTop = (i: number) => plotTop + i * rowH + (i ? 12 : 0);
  const plotBottom = rowTop(rows.length) + 4;
  const H = plotBottom + 52;

  const { out, t } = svgWriter();
  const progress = (c: ConfigurationTrend) =>
    c.trend ? (1 + c.trend.overall.at(-1)!) / (1 + c.trend.overall[0]) - 1 : null;
  out.push(
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}" role="img" aria-labelledby="t d">`,
  );
  out.push(
    `<title id="t">PR benchmark history: ${escapeXml(
      configurations
        .map((c) =>
          c.trend
            ? `${c.name} ${formatChange(progress(c))} run time over ${c.trend.runs.length} runs`
            : `${c.name} unavailable`,
        )
        .join("; "),
    )}</title>`,
  );
  out.push(
    `<desc id="d">${escapeXml(
      rows
        .map(
          (r) =>
            `${r.label}: ` +
            configurations
              .map((c, i) => `${c.name} ${r.cells[i]?.map(formatChange).join(", ") ?? "—"}`)
              .join("; "),
        )
        .join(" | "),
    )}</desc>`,
  );
  out.push(chartStyles);
  out.push(`<style>
  .main { stroke-dasharray: 3 3; } .on-main { fill: #8d8c86; }
  @media (prefers-color-scheme: dark) { .on-main { fill: #6f6e69; } }
</style>`);
  out.push(`<rect class="panel" x="0.5" y="0.5" width="${W - 1}" height="${H - 1}" rx="12"/>`);

  // The headline is the movement within the PR; the gap to main is the
  // comparison chart's headline and each Overall line's end.
  t("ink strong", PAD, 36, title.input);
  t("muted sub", PAD, 51, title.sub);
  configurations.forEach((c, i) => {
    if (!c.trend) {
      drawHeader(t, cols[i], "—", "muted", c.name, "no history");
      return;
    }
    const p = progress(c)!;
    const runs = c.trend.runs;
    drawHeader(t, cols[i], formatChange(p), side(p) ?? "ink", c.name, `${runs.length} runs`);
  });
  out.push(
    `<line class="rule" x1="${PAD}" x2="${W - PAD}" y1="${heroBottom}" y2="${heroBottom}" stroke-width="1"/>`,
  );
  if (rows.length > 1) {
    const ruleY = plotTop + rowH + 6;
    out.push(
      `<line class="rule" x1="${PAD}" x2="${W - PAD}" y1="${ruleY}" y2="${ruleY}" stroke-width="1"/>`,
    );
  }

  rows.forEach((r, i) => {
    const cy = rowTop(i) + rowH / 2;
    drawName(out, t, r.name, cy);
    // A row's scale spans its range in every configuration and main, but
    // never less than MIN_SWING, so a gap below the selection threshold does
    // not fill the row and look as large as one selected for its size.
    const all = r.cells.flatMap((cell) => cell ?? []);
    const low = Math.min(0, ...all);
    const high = Math.max(0, ...all);
    const span = Math.max(high - low, MIN_SWING);
    const mid = (high + low) / 2;
    const ly = (v: number) => cy + 3 - ((v - mid) / span) * bandH;
    r.cells.forEach((changes, j) => {
      const col = cols[j];
      if (!changes) {
        t("muted sub", col.right, cy + 4, "—", ' text-anchor="end"');
        return;
      }
      const lineL = col.left + 6;
      const lineR = col.plotR - 8;
      const latest = changes.length - 1;
      const x = (k: number) => (latest ? lineL + (k * (lineR - lineL)) / latest : lineR);
      out.push(
        `<line class="axis main" x1="${lineL}" x2="${lineR.toFixed(1)}" y1="${ly(0).toFixed(1)}" y2="${ly(0).toFixed(1)}" stroke-width="1"/>`,
      );
      // Split each segment where it crosses main so each piece has one colour.
      const piece = (x0: number, v0: number, x1: number, v1: number) => {
        const cls = side(v0 + v1);
        out.push(
          `<line class="${cls ? `stem-${cls}` : "axis"}" x1="${x0.toFixed(1)}" y1="${ly(v0).toFixed(1)}" x2="${x1.toFixed(1)}" y2="${ly(v1).toFixed(1)}" stroke-width="2" stroke-linecap="round"/>`,
        );
      };
      changes.slice(1).forEach((v1, k) => {
        const v0 = changes[k];
        if (v0 * v1 < 0) {
          const xc = x(k) + ((x(k + 1) - x(k)) * v0) / (v0 - v1);
          piece(x(k), v0, xc, 0);
          piece(xc, 0, x(k + 1), v1);
        } else piece(x(k), v0, x(k + 1), v1);
      });
      const end = changes[latest];
      const major = Math.abs(end) >= MAJOR;
      if (latest)
        out.push(
          `<circle class="${side(changes[0]) ?? "on-main"} ring" cx="${x(0).toFixed(1)}" cy="${ly(changes[0]).toFixed(1)}" r="2.5" stroke-width="1.5"/>`,
        );
      out.push(
        `<circle class="${side(end) ?? "on-main"} ring" cx="${x(latest).toFixed(1)}" cy="${ly(end).toFixed(1)}" r="${major ? 5.5 : 4}" stroke-width="2"/>`,
      );
      t(
        `val ink${r.aggregate || major ? " strong" : ""}`,
        col.right,
        cy + 4,
        formatChange(end),
        ' text-anchor="end"',
      );
    });
  });

  const legendY = plotBottom + 30;
  out.push(
    `<line class="axis main" x1="${PAD}" x2="${PAD + 14}" y1="${legendY - 4}" y2="${legendY - 4}" stroke-width="1"/>`,
  );
  // Builds of one tracking run share their commits, so the range is usually
  // one; a build with a failed run lists its own.
  const bases = [...new Set(measured.map((c) => short(c.trend!.base)))];
  const ranges = [
    ...new Set(
      measured.map((c) => `${short(c.trend!.runs[0].hash)} → ${short(c.trend!.runs.at(-1)!.hash)}`),
    ),
  ];
  const baseLabel = `${base} ${bases.join(", ")} · runs ${ranges.join(", ")}`;
  t("muted sub", PAD + 20, legendY, baseLabel);
  t(
    "muted sub",
    W - PAD,
    legendY,
    `value: latest run against ${base} · one scale per row`,
    ' text-anchor="end"',
  );
  out.push("</svg>");
  return out.join("\n");
}
