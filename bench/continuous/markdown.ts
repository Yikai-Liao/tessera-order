// SPDX-License-Identifier: BSD-2-Clause

import {
  attribution,
  type CaseRow,
  type Configuration,
  caseRows,
  changedRows,
  geomean,
  isSlower,
  modeName,
  modes,
  severity,
} from "./matrix.ts";
import {
  formatChange,
  formatTime,
  isMajor,
  MAJOR,
  type ReportSummary,
  type Row,
  short,
  type Verdict,
} from "./report.ts";

const BENCHER = "https://bencher.dev";
const API = "https://api.bencher.dev/v0";

interface RenderOptions {
  language: string;
  // The build configuration, such as "Portable SIMD"; it names the check.
  configuration?: string;
  runUrl: string;
  baseRef?: string;
  // Why the promised comparison is absent although the cases were measured.
  unavailable?: string;
  chartRef?: string | null;
  trendRef?: string | null;
  trendUnavailable?: boolean;
  imagesUnavailable?: boolean;
}

interface FailureOptions {
  language: string;
  configuration?: string;
  hash: string;
  runUrl: string;
  reason: string;
}

// A configuration as the report sees it: measured, or why not.
export interface ConfigurationResult extends Configuration {
  outcome: string;
  reason?: string;
  // Why the promised comparison is absent; the cases were still measured.
  unavailable?: string;
}

function plural(n: number, word: string) {
  return `${n} ${word}${n === 1 ? "" : "s"}`;
}

// The headline leads with what the reader must act on, then what to
// celebrate. It is also the check title in the PR's check list.
export function headline(s: ReportSummary, unavailable = false) {
  // Cases without a baseline are expected only where the suite grew; a
  // missing comparison for every case is a failed promise, not a new suite.
  if (unavailable) return "no comparable baseline";
  const parts = [];
  if (s.regressions) parts.push(`${s.regressions} slower`);
  if (s.improvements) parts.push(`${s.improvements} faster`);
  if (s.missing) parts.push(`${s.missing} new`);
  // Without a single compared case there is nothing to call unchanged.
  if (!s.regressions && !s.improvements)
    parts.push(s.compared ? "no regression alert" : "no baseline");
  return parts.join(", ");
}

// A failed promise, in the place a reader looks for the verdict.
function unavailableBox(reason: string) {
  return `> [!WARNING]\n> **No comparable baseline:** ${reason}. The cases below were measured but not compared.  `;
}

function metric(value: number | null, relative = false) {
  return relative ? (value == null ? "—" : `${value.toFixed(4)}×`) : formatTime(value);
}

function caseLine(r: Row, relative = false) {
  return `${r.label}, ${metric(r.baseline, relative)} → **${metric(r.value, relative)}**`;
}

// GitHub alert blocks render as coloured boxes, so the verdict is visible
// before anyone reads a table. Regressions come first: they need action.
function callouts(s: ReportSummary) {
  const out = [];
  const major = (verdict: Verdict) =>
    s.rows.filter((r) => r.verdict === verdict && isMajor(r)).length;
  if (s.worst) {
    const n = major("regression");
    out.push(
      [
        "> [!CAUTION]",
        `> **${plural(s.regressions, "case")} slower${n ? `, ${n} by ${MAJOR * 100}% or more` : ""}.** ` +
          `Worst: ${caseLine(s.worst, s.relative)}, **${formatChange(s.worst.change)}**.  `,
      ].join("\n"),
    );
  }
  if (s.best) {
    const n = major("improvement");
    out.push(
      [
        "> [!TIP]",
        `> **${plural(s.improvements, "case")} faster${n ? `, ${n} by ${MAJOR * 100}% or more` : ""}.** ` +
          `Best: ${caseLine(s.best, s.relative)}, **${formatChange(s.best.change)}**. Nice work.  `,
      ].join("\n"),
    );
  }
  return out.join("\n\n");
}

function overall(s: ReportSummary, baseRef: string) {
  if (s.geomean == null) return null;
  const g = s.geomean;
  return `**Overall run time ${formatChange(g - 1)}** against \`${baseRef}\`, the geometric mean over ${s.compared} cases.`;
}

// Public perf pages need no login, unlike the console links in Bencher's own
// comment. A PR page plots the PR head beside its start point.
function perfUrl(s: ReportSummary, benchmarks: string[], base = `${BENCHER}/perf/${s.project}`) {
  const query = new URLSearchParams({
    branches: s.history.map((source) => source.branch).join(","),
    heads: s.history.map((source) => source.head).join(","),
    testbeds: s.testbed.uuid,
    benchmarks: benchmarks.join(","),
    measures: [...new Set(s.rows.map((r) => r.measure))].join(","),
  });
  return `${base}?${query}`;
}

function links(s: ReportSummary) {
  if (!s.project || !s.uuid) return [];
  const api = `${API}/projects/${s.project}`;
  const alerted = s.rows.filter((r) => r.verdict === "regression" || r.verdict === "improvement");
  const parts = [
    `[Report](${BENCHER}/perf/${s.project}/reports/${s.uuid})`,
    `[JSON](${api}/reports/${s.uuid})`,
  ];
  if (alerted.length && !s.relative)
    parts.push(
      `[History](${perfUrl(
        s,
        alerted.map((r) => r.benchmark),
        `${api}/perf`,
      )})`,
    );
  return parts;
}

function commits(s: ReportSummary, baseRef: string) {
  return s.baseHash ? `${short(s.hash)} → \`${baseRef}\` ${short(s.baseHash)}` : short(s.hash);
}

function unavailable(options: RenderOptions) {
  const parts = [];
  if (options.trendUnavailable) parts.push(`[Trend unavailable](${options.runUrl})`);
  if (options.imagesUnavailable) parts.push(`[Images unavailable](${options.runUrl})`);
  return parts;
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

function inputCell(p: Row["parts"], s?: ReportSummary, r?: Row) {
  const input = cell(p.input + (p.seed && p.seed !== "s1" ? ` (seed ${p.seed.slice(1)})` : ""));
  return s && r && !s.relative ? `[${input}](${perfUrl(s, [r.benchmark])} "${r.name}")` : input;
}

function caseCells(r: Row, s?: ReportSummary) {
  const p = r.parts;
  return [inputCell(p, s, r), cell(p.type) || "—", p.n || "—", cell(modeName(p.mode)) || "—"].join(
    " | ",
  );
}

function alertTable(rows: Row[], s: ReportSummary) {
  const lines = [
    "| Input | Type | N | Mode | Baseline | Current | Change |",
    "|---|---|---:|---|---:|---:|---:|",
  ];
  for (const r of rows) {
    lines.push(
      `| ${caseCells(r, s)} | ${metric(r.baseline, s.relative)} | ${metric(r.value, s.relative)} | ${changeCell(r)} |`,
    );
  }
  return lines.join("\n");
}

const STATUS = { regression: "slower", improvement: "faster", missing: "new", within: "" };

function fullTable(rows: Row[], relative = false) {
  const lines = [
    `| Input | Type | N | Mode | Baseline | ${relative ? "Median interval" : "Limits"} | Current | Change | Alert | ID |`,
    "|---|---|---:|---|---:|---:|---:|---:|---|---|",
  ];
  for (const r of rows) {
    const limits =
      r.lower == null ? "—" : `${metric(r.lower, relative)} – ${metric(r.upper, relative)}`;
    lines.push(
      `| ${caseCells(r)} | ${metric(r.baseline, relative)} | ${limits} | ${metric(r.value, relative)} | ` +
        `${formatChange(r.change)} | ${STATUS[r.verdict]} | \`${r.name}\` |`,
    );
  }
  return lines.join("\n");
}

// One configuration on its own: the check of that configuration, which a
// reader opens from the check list without the PR comment around it.
export function render(s: ReportSummary, options: RenderOptions) {
  const {
    language,
    configuration,
    baseRef = "main",
    unavailable: noBaseline,
    chartRef,
    trendRef,
    runUrl,
  } = options;
  const slower = s.rows.filter((r) => r.verdict === "regression");
  const faster = s.rows.filter((r) => r.verdict === "improvement");
  const out = [
    `### Benchmarks · ${[language, configuration, headline(s, Boolean(noBaseline))].filter(Boolean).join(" · ")}`,
  ];
  const boxes = callouts(s);
  if (boxes) out.push("", boxes);
  if (noBaseline) out.push("", unavailableBox(noBaseline));
  // The charts hold every configuration; the check's own column is named in
  // their header.
  if (chartRef) out.push("", `![Changes against ${baseRef}](${chartRef})`);
  if (trendRef) out.push("", `![PR benchmark history](${trendRef})`);
  const summaryLine = overall(s, baseRef);
  if (summaryLine) out.push("", summaryLine);
  if (slower.length) out.push("", "#### Slower", "", alertTable(slower, s));
  if (faster.length) out.push("", "#### Faster", "", alertTable(faster, s));
  // Measured but uncompared cases are not new cases; the warning says what is
  // missing instead.
  if (s.missing && !noBaseline)
    out.push("", `<sub>${plural(s.missing, "new case")} without a baseline.</sub>`);
  out.push(
    "",
    `<details><summary>All ${s.rows.length} cases</summary>`,
    "",
    fullTable(s.rows, s.relative),
    "",
    "</details>",
  );
  const footer = [
    commits(s, baseRef),
    `\`${s.testbed.name}\``,
    `[Run](${runUrl})`,
    ...links(s),
    ...unavailable(options),
  ];
  out.push("", `<sub>${footer.join(" · ")}</sub>`);
  return out.join("\n");
}

export function renderFailure({ language, configuration, hash, runUrl, reason }: FailureOptions) {
  return [
    `### Benchmarks · ${[language, configuration, "measurement failed"].filter(Boolean).join(" · ")}`,
    "",
    `**${short(hash)}** was not measured: ${reason}. See the [workflow run](${runUrl}).`,
  ].join("\n");
}

// The PR comment and job summary: every configuration in one report.
//
// Configurations are columns and cases are rows, everywhere: the overview,
// the charts and the changed-case table. A case is named once however many
// builds measured it, and each alert states whether it is shared by every
// build or confined to some, which is the first clue to its cause. API
// variants (stable, later unstable or parallel) are modes of the
// case ID and become row groups, so the report grows by rows, not by
// sections or charts.

function signed(change: number | null) {
  return formatChange(change);
}

// The verdict across configurations, counted by distinct case: a case slower
// in two builds is one slower case.
function combinedHeadline(configurations: ConfigurationResult[], rows: CaseRow[]) {
  const slower = rows.filter(isSlower).length;
  const faster = rows.length - slower;
  const parts = [];
  if (slower) parts.push(`${slower} slower`);
  if (faster) parts.push(`${faster} faster`);
  const measured = configurations.filter((c) => c.summary);
  if (!slower && !faster) {
    parts.push(measured.some((c) => c.summary!.compared) ? "no significant change" : "no baseline");
  }
  const fresh = caseRows(configurations).filter((row) =>
    row.cells.some((cell) => cell?.baseline === null),
  ).length;
  if (fresh) parts.push(`${fresh} new`);
  for (const c of configurations) {
    if (!c.summary) parts.push(`${c.name} ${c.outcome === "cancelled" ? "cancelled" : "failed"}`);
  }
  return parts.join(", ");
}

function overviewCell(summary: ReportSummary, mode?: string) {
  const g = geomean(summary, mode);
  const rows = summary.rows.filter((r) => mode === undefined || r.parts.mode === mode);
  const slower = rows.filter((r) => r.verdict === "regression").length;
  const faster = rows.filter((r) => r.verdict === "improvement").length;
  const fresh = rows.filter((r) => r.baseline === null).length;
  const counts = [
    slower ? `**${slower} slower**` : "",
    faster ? `${faster} faster` : "",
    fresh ? `${fresh} new` : "",
  ]
    .filter(Boolean)
    .join(" · ");
  const value = g ? signed(g.change) : "—";
  return counts ? `${value} · ${counts}` : value;
}

// Rows are the whole suite and then each mode; columns are configurations.
// A cell is the geometric mean of current/baseline over its compared cases
// with that configuration's alert counts, so a change confined to one API
// or one build stands out without reading any case.
function overview(configurations: ConfigurationResult[]) {
  const header = ["", ...configurations.map((c) => c.name)];
  const lines = [
    `| ${header.join(" | ")} |`,
    `|---|${configurations.map(() => "---:").join("|")}|`,
  ];
  const row = (label: string, mode?: string) =>
    lines.push(
      `| ${label} | ${configurations
        .map((c) =>
          c.summary
            ? overviewCell(c.summary, mode)
            : c.outcome === "cancelled"
              ? "cancelled"
              : "failed",
        )
        .join(" | ")} |`,
    );
  const all = modes(configurations);
  row("**All cases**");
  if (all.length > 1) for (const mode of all) row(modeName(mode), mode);
  return lines.join("\n");
}

const cellText = (r: Row | undefined, bold = true) =>
  !r
    ? "—"
    : r.change == null
      ? "new"
      : r.verdict === "within"
        ? signed(r.change)
        : bold
          ? `**${signed(r.change)}**`
          : signed(r.change);

// The worst slowdowns across configurations, one line per case with every
// configuration's change, so a reader sees at once whether it is shared.
function caution(configurations: ConfigurationResult[], rows: CaseRow[]) {
  const slower = rows.filter(isSlower).sort((a, b) => severity(b) - severity(a));
  if (!slower.length) return null;
  const groups = new Map<string | null, number>();
  for (const row of slower) {
    const key = attribution(
      configurations,
      row.cells.map((c) => c?.verdict === "regression"),
    );
    groups.set(key, (groups.get(key) ?? 0) + 1);
  }
  const where = [...groups]
    .filter(([key]) => key)
    .map(([key, n]) => `${n} in ${key}`)
    .join(", ");
  const shown = 3;
  const out = [
    "> [!CAUTION]",
    `> **${plural(slower.length, "case")} slower**${where ? `: ${where}` : ""}.`,
  ];
  for (const row of slower.slice(0, shown)) {
    out.push(
      `> - ${row.label}: ` +
        configurations.map((c, i) => `${c.name} ${cellText(row.cells[i])}`).join(", "),
    );
  }
  if (slower.length > shown)
    out.push(`> - and ${slower.length - shown} more in the changed cases below.`);
  return out.join("\n");
}

// A speedup gets a tip box of its own, a single line below any caution box.
function speedups(configurations: ConfigurationResult[], rows: CaseRow[]) {
  const faster = rows.filter((row) => !isSlower(row)).sort((a, b) => severity(a) - severity(b));
  if (!faster.length) return null;
  const groups = new Map<string | null, number>();
  for (const row of faster) {
    const key = attribution(
      configurations,
      row.cells.map((c) => c?.verdict === "improvement"),
    );
    groups.set(key, (groups.get(key) ?? 0) + 1);
  }
  const where = [...groups]
    .filter(([key]) => key)
    .map(([key, n]) => `${n} in ${key}`)
    .join(", ");
  const best = faster[0];
  const i = best.cells.findIndex(
    (c) => c?.verdict === "improvement" && c.change === severity(best),
  );
  return [
    "> [!TIP]",
    `> **${plural(faster.length, "case")} faster**${where ? `: ${where}` : ""}. ` +
      `Best: ${best.label}, ${configurations[i].name} **${signed(severity(best))}**. Nice work.`,
  ].join("\n");
}

function changedTable(configurations: ConfigurationResult[], rows: CaseRow[]) {
  const head = ["Input", "Type", "N", "Mode", ...configurations.map((c) => c.name)];
  const lines = [
    `| ${head.join(" | ")} |`,
    `|---|---|---:|---|${configurations.map(() => "---:").join("|")}|`,
  ];
  for (const row of rows) {
    const first = row.cells.findIndex((c) => c);
    const s = configurations[first].summary!;
    const cells = row.cells.map((r, index) =>
      !r
        ? "—"
        : r.change == null
          ? "new"
          : `${cellText(r)} <sub>${metric(r.value, configurations[index].summary?.relative)}</sub>`,
    );
    lines.push(
      `| ${inputCell(row.parts, s, row.cells[first])} | ${cell(row.parts.type) || "—"} | ${row.parts.n || "—"} | ` +
        `${cell(modeName(row.parts.mode))} | ${cells.join(" | ")} |`,
    );
  }
  return lines.join("\n");
}

export interface CombinedOptions {
  language: string;
  hash: string;
  runUrl: string;
  baseRef?: string;
  chartRef?: string | null;
  trendRef?: string | null;
  trendUnavailable?: boolean;
  imagesUnavailable?: boolean;
}

export function renderCombined(configurations: ConfigurationResult[], options: CombinedOptions) {
  const { language, hash, runUrl, baseRef = "main", chartRef, trendRef } = options;
  // A build that was not measured stays in the overview and gets a line of
  // its own; the case lines, table and charts cover only measured builds, so
  // "only" never means "the other build failed".
  const measured = configurations.filter((c) => c.summary);
  const rows = changedRows(measured);
  const out = [`### Benchmarks · ${language} · ${combinedHeadline(configurations, rows)}`];
  out.push("", overview(configurations));
  for (const c of configurations) {
    if (!c.summary)
      out.push(
        "",
        `**${c.name}** was not measured: ${c.reason ?? "the measurement failed"}. See the [workflow run](${runUrl}).`,
      );
    else if (c.unavailable)
      out.push(
        "",
        `**${c.name}** was measured but not compared: ${c.unavailable}. See the [workflow run](${runUrl}).`,
      );
  }
  const boxes = [caution(measured, rows), speedups(measured, rows)].filter(Boolean);
  if (boxes.length) out.push("", boxes.join("\n\n"));
  if (chartRef) out.push("", `![Changes against ${baseRef}](${chartRef})`);
  if (trendRef) out.push("", `![PR benchmark history](${trendRef})`);
  if (rows.length) {
    out.push(
      "",
      `<details><summary>${plural(rows.length, "changed case")}</summary>`,
      "",
      changedTable(measured, rows),
      "",
      "</details>",
    );
  }
  const fresh = measured.filter((c) => c.summary!.missing && !c.unavailable);
  if (fresh.length) {
    const same =
      fresh.length === measured.length &&
      fresh.every((c) => c.summary!.missing === fresh[0].summary!.missing);
    out.push(
      "",
      same && measured.length > 1
        ? `<sub>${plural(fresh[0].summary!.missing, "new case")} without a baseline in each build.</sub>`
        : `<sub>New without a baseline: ${fresh.map((c) => `${c.summary!.missing} in ${c.name}`).join(", ")}.</sub>`,
    );
  }
  if (measured.length) {
    out.push("", "<details><summary>All cases, by build</summary>", "");
    for (const c of measured)
      out.push(
        `**${c.name}** · \`${c.summary!.testbed.name}\``,
        "",
        fullTable(c.summary!.rows, c.summary!.relative),
        "",
      );
    out.push("</details>");
  }
  const first = measured[0]?.summary;
  const footer = [
    first ? commits(first, baseRef) : short(hash),
    `[Run](${runUrl})`,
    ...measured.map(
      (c) => `${c.name}: \`${c.summary!.testbed.name}\` ${links(c.summary!).join(" ")}`,
    ),
    ...unavailable({ ...options, language }),
  ];
  out.push("", `<sub>${footer.join(" · ")}</sub>`);
  return out.join("\n");
}
