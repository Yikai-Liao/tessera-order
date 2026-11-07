// SPDX-License-Identifier: BSD-2-Clause

import { formatChange, formatTime, isMajor, MAJOR, short, type ReportSummary, type Row, type Verdict } from "./report.ts";

const BENCHER = "https://bencher.dev";
const API = "https://api.bencher.dev/v0";

interface RenderOptions {
  language: string;
  runUrl: string;
  baseRef?: string;
  chartRef?: string | null;
  trendRef?: string | null;
  trendUnavailable?: boolean;
  imagesUnavailable?: boolean;
}

interface FailureOptions {
  language: string;
  hash: string;
  runUrl: string;
  reason: string;
}

export interface ConfigurationResult {
  name: string;
  outcome: string;
  summary: ReportSummary | null;
}

export function renderOverview(language: string, baseRef: string, configurations: ConfigurationResult[]) {
  const out = [
    `### Benchmarks · ${language}`,
    "",
    `Each configuration is compared with its own baseline on \`${baseRef}\`.`,
    "",
    "| Configuration | Status | Slower | Faster | New | Overall run time |",
    "|---|---|---:|---:|---:|---:|",
  ];
  for (const { name, outcome, summary: s } of configurations) {
    if (s) {
      out.push(`| ${name} | ${s.regressions ? "Possible regressions" : "Passed"} | ` +
        `${s.regressions} | ${s.improvements} | ${s.missing} | ${formatChange(s.geomean === null ? null : s.geomean - 1)} |`);
    } else {
      out.push(`| ${name} | ${outcome === "cancelled" ? "Cancelled" : "Measurement failed"} | — | — | — | — |`);
    }
  }
  return out.join("\n");
}

function plural(n: number, word: string) {
  return `${n} ${word}${n === 1 ? "" : "s"}`;
}

// The headline leads with what the reader must act on, then what to
// celebrate. It is also the check title in the PR's check list.
export function headline(s: ReportSummary) {
  const parts = [];
  if (s.regressions) parts.push(`${s.regressions} slower`);
  if (s.improvements) parts.push(`${s.improvements} faster`);
  if (s.missing) parts.push(`${s.missing} new`);
  if (!s.regressions && !s.improvements) parts.push("no significant change");
  return parts.join(", ");
}

function caseLine(r: Row) {
  return `${r.label}, ${formatTime(r.baseline)} → **${formatTime(r.value)}**`;
}

// GitHub alert blocks render as coloured boxes, so the verdict is visible
// before anyone reads a table. Regressions come first: they need action.
function callouts(s: ReportSummary) {
  const out = [];
  const major = (verdict: Verdict) => s.rows.filter(r => r.verdict === verdict && isMajor(r)).length;
  if (s.worst) {
    const n = major("regression");
    out.push([
      "> [!CAUTION]",
      `> **${plural(s.regressions, "case")} slower${n ? `, ${n} by ${MAJOR * 100}% or more` : ""}.** ` +
        `Worst: ${caseLine(s.worst)}, **${formatChange(s.worst.change)}**.  `,
    ].join("\n"));
  }
  if (s.best) {
    const n = major("improvement");
    out.push([
      "> [!TIP]",
      `> **${plural(s.improvements, "case")} faster${n ? `, ${n} by ${MAJOR * 100}% or more` : ""}.** ` +
        `Best: ${caseLine(s.best)}, **${formatChange(s.best.change)}**. Nice work.  `,
    ].join("\n"));
  }
  return out.join("\n\n");
}

function overall(s: ReportSummary, baseRef: string) {
  if (s.geomean == null) return null;
  const g = s.geomean;
  return `**Overall run time ${formatChange(g - 1)}** against \`${baseRef}\`, the geometric mean over ${s.compared} cases.`;
}

// Public perf pages need no login, unlike the console links in Bencher's own
// comment. A PR page plots the PR head beside its start point on main.
function perfUrl(s: ReportSummary, benchmarks: string[], base = `${BENCHER}/perf/${s.project}`) {
  const query = new URLSearchParams({
    branches: s.history.map(source => source.branch).join(","),
    heads: s.history.map(source => source.head).join(","),
    testbeds: s.testbed.uuid, benchmarks: benchmarks.join(","),
    measures: [...new Set(s.rows.map(r => r.measure))].join(","),
  });
  return `${base}?${query}`;
}

function footer(s: ReportSummary, options: RenderOptions, alerted: Row[]) {
  const { runUrl, baseRef = "main", trendUnavailable, imagesUnavailable } = options;
  const api = `${API}/projects/${s.project}`;
  const parts = [
    s.baseHash ? `${short(s.hash)} → \`${baseRef}\` ${short(s.baseHash)}` : short(s.hash),
    `\`${s.testbed.name}\``,
    `[Run](${runUrl})`,
    `[Report](${BENCHER}/perf/${s.project}/reports/${s.uuid})`,
    `[JSON](${api}/reports/${s.uuid})`,
  ];
  if (alerted.length) parts.push(`[History](${perfUrl(s, alerted.map(r => r.benchmark), `${api}/perf`)})`);
  if (trendUnavailable) parts.push(`[Trend unavailable](${runUrl})`);
  if (imagesUnavailable) parts.push(`[Images unavailable](${runUrl})`);
  return `<sub>${parts.join(" · ")}</sub>`;
}

// Every change is the signed change in run time: minus is less time.
function changeCell(r: Row) {
  if (r.change == null) return "—";
  const text = formatChange(r.change);
  return isMajor(r) ? `**${text}**` : text;
}

function cell(text: string) {
  return text.replace(/\|/g, "\\|").replace(/[\r\n]/g, " ");
}

function caseCells(r: Row, s?: ReportSummary) {
  const p = r.parts;
  const input = cell(p.input + (p.seed && p.seed !== "s1" ? ` (seed ${p.seed.slice(1)})` : ""));
  const linked = s ? `[${input}](${perfUrl(s, [r.benchmark])} "${r.name}")` : input;
  return [linked, cell(p.type) || "—", p.n || "—", cell(p.mode) || "—"].join(" | ");
}

function alertTable(rows: Row[], s: ReportSummary) {
  const lines = ["| Input | Type | N | Mode | Baseline | Current | Change |", "|---|---|---:|---|---:|---:|---:|"];
  for (const r of rows) {
    lines.push(`| ${caseCells(r, s)} | ${formatTime(r.baseline)} | ${formatTime(r.value)} | ${changeCell(r)} |`);
  }
  return lines.join("\n");
}

const STATUS = { regression: "slower", improvement: "faster", missing: "new", within: "" };

function fullTable(rows: Row[]) {
  const lines = ["| Input | Type | N | Mode | Baseline | Limits | Current | Change | Alert | ID |",
    "|---|---|---:|---|---:|---:|---:|---:|---|---|"];
  for (const r of rows) {
    const limits = r.lower == null ? "—" : `${formatTime(r.lower)} – ${formatTime(r.upper)}`;
    lines.push(`| ${caseCells(r)} | ${formatTime(r.baseline)} | ${limits} | ${formatTime(r.value)} | ` +
      `${formatChange(r.change)} | ${STATUS[r.verdict]} | \`${r.name}\` |`);
  }
  return lines.join("\n");
}

export function render(s: ReportSummary, options: RenderOptions) {
  const { language, baseRef = "main", chartRef, trendRef } = options;
  const slower = s.rows.filter(r => r.verdict === "regression");
  const faster = s.rows.filter(r => r.verdict === "improvement");
  const out = [`### Benchmarks · ${language} · ${headline(s)}`];
  // The call-outs name the worst and best case; the charts follow: the
  // comparison with main, then the PR's own history, whose headline names
  // it. Text and tables after them carry the exact numbers.
  const boxes = callouts(s);
  if (boxes) out.push("", boxes);
  if (chartRef) out.push("", `![Changes against main](${chartRef})`);
  if (trendRef) out.push("", `![PR benchmark history](${trendRef})`);
  const summaryLine = overall(s, baseRef);
  if (summaryLine) out.push("", summaryLine);
  if (slower.length) out.push("", "#### Slower", "", alertTable(slower, s));
  if (faster.length) out.push("", "#### Faster", "", alertTable(faster, s));
  if (s.missing) out.push("", `<sub>${plural(s.missing, "new case")} without a baseline.</sub>`);
  out.push("", `<details><summary>All ${s.rows.length} cases</summary>`, "", fullTable(s.rows), "", "</details>");
  out.push("", footer(s, options, [...slower, ...faster]));
  return out.join("\n");
}

export function renderFailure({ language, hash, runUrl, reason }: FailureOptions) {
  return [
    `### Benchmarks · ${language} · measurement failed`,
    "",
    `**${short(hash)}** was not measured: ${reason}. See the [workflow run](${runUrl}).`,
  ].join("\n");
}
