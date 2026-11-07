// SPDX-License-Identifier: BSD-2-Clause

import { formatChange, formatTime, isMajor, MAJOR, secondary, short, type ReportSummary, type Row, type Verdict } from "./report.ts";

const BENCHER = "https://bencher.dev";
const API = "https://api.bencher.dev/v0";

interface RenderOptions {
  language: string;
  runUrl: string;
  baseRef?: string;
  chartRef?: string | null;
}

interface FailureOptions {
  language: string;
  hash: string;
  runUrl: string;
  reason: string;
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
      "> Rerun, or confirm with the A/A and placement controls, before merging.",
    ].join("\n"));
  }
  if (s.best) {
    const n = major("improvement");
    out.push([
      "> [!TIP]",
      `> **${plural(s.improvements, "case")} faster${n ? `, ${n} by ${MAJOR * 100}% or more` : ""}.** ` +
        `Best: ${caseLine(s.best)}, **${formatChange(s.best.change)}**. Nice work.  `,
      "> Confirm with the full protocol before citing the speedup.",
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

// The public API needs no login or token, so a reader or an agent can fetch
// the data directly; workflow artifacts would need a GitHub login and expire.
function dataLinks(s: ReportSummary, alerted: Row[]) {
  const api = `${API}/projects/${s.project}`;
  const links = [`[report JSON](${api}/reports/${s.uuid})`];
  if (alerted.length) {
    links.push(`[history JSON of alerted cases](${perfUrl(s, alerted.map(r => r.benchmark), `${api}/perf`)})`);
  }
  return `Data, without login: ${links.join(" · ")}. ` +
    "The report holds every case with its baseline and limits; the history holds each sample on `main` and this branch.";
}

// Every change is the signed change in run time: minus is less time.
function changeCell(r: Row) {
  if (r.change == null) return "—";
  const text = formatChange(r.change);
  return isMajor(r) ? `**${text}**` : text;
}

// The case name links to its public history chart; the link title carries
// the benchmark ID for hover and for agents reading the Markdown.
// The input leads; type and n follow in small text, as in the chart.
function caseCell(r: Row) {
  const rest = [secondary(r), r.parts.extra].filter(Boolean).join(" · ");
  return r.parts.input + (rest ? ` <sub>${rest}</sub>` : "");
}

function alertTable(rows: Row[], s: ReportSummary) {
  const lines = ["| Case | Baseline | Current | Change |", "|---|---:|---:|---:|"];
  for (const r of rows) {
    lines.push(`| [${caseCell(r)}](${perfUrl(s, [r.benchmark])} "${r.name}") | ` +
      `${formatTime(r.baseline)} | ${formatTime(r.value)} | ${changeCell(r)} |`);
  }
  return lines.join("\n");
}

const STATUS = { regression: "slower", improvement: "faster", missing: "new", within: "" };

function fullTable(rows: Row[]) {
  const lines = ["| Case | Baseline | Limits | Current | Change | Alert | ID |",
    "|---|---:|---:|---:|---:|---|---|"];
  for (const r of rows) {
    const limits = r.lower == null ? "—" : `${formatTime(r.lower)} – ${formatTime(r.upper)}`;
    lines.push(`| ${caseCell(r)} | ${formatTime(r.baseline)} | ${limits} | ${formatTime(r.value)} | ` +
      `${formatChange(r.change)} | ${STATUS[r.verdict]} | \`${r.name}\` |`);
  }
  return lines.join("\n");
}

export function render(s: ReportSummary, { language, runUrl, baseRef = "main", chartRef = null }: RenderOptions) {
  const slower = s.rows.filter(r => r.verdict === "regression");
  const faster = s.rows.filter(r => r.verdict === "improvement");
  const missing = s.rows.filter(r => r.verdict === "missing");
  const base = s.baseHash ? `\`${baseRef}\` ${short(s.baseHash)}` : "recent history";
  const out = [`### Benchmarks · ${language} · ${headline(s)}`];
  const boxes = callouts(s);
  if (boxes) out.push("", boxes);
  if (chartRef && (slower.length || faster.length)) {
    out.push("", `![Change from baseline for each alerted case](${chartRef})`);
  }
  const summaryLine = overall(s, baseRef);
  out.push("", [summaryLine,
    `${short(s.hash)} against ${base} on Testbed \`${s.testbed.name}\` · ` +
      `[workflow run](${runUrl}) · [Bencher report](${BENCHER}/perf/${s.project}/reports/${s.uuid})`,
  ].filter(Boolean).join("  \n"));
  if (slower.length) out.push("", `#### Slower`, "", alertTable(slower, s));
  if (faster.length) out.push("", `#### Faster`, "", alertTable(faster, s));
  if (missing.length) {
    out.push("", `New, without a baseline on \`${baseRef}\`: ` +
      missing.map(r => `${r.label} (${formatTime(r.value)})`).join(", ") + ".");
  }
  out.push("", dataLinks(s, [...slower, ...faster]));
  out.push("", `<details><summary>All ${s.rows.length} cases</summary>`, "", fullTable(s.rows), "", "</details>");
  const m = s.model;
  const method = m
    ? `Baseline: the mean of up to ${m.max_sample_size} samples on \`${baseRef}\`, at least ${m.min_sample_size}. ` +
      `Limits: a ${m.test.replace("_", "-")} at ${m.lower_boundary}.`
    : "No threshold applies to this branch yet.";
  out.push("", `<sub>${method} An alert marks a case to investigate with the ` +
    "[benchmark protocol](https://github.com/Yikai-Liao/tessera-sort/blob/main/docs/development/benchmarking.md); " +
    "it is not evidence for a performance claim. Earlier reports on this PR are collapsed as outdated.</sub>");
  return out.join("\n");
}

export function renderFailure({ language, hash, runUrl, reason }: FailureOptions) {
  return [
    `### Benchmarks · ${language} · measurement failed`,
    "",
    `**${short(hash)}** was not measured: ${reason}. See the [workflow run](${runUrl}).`,
  ].join("\n");
}
