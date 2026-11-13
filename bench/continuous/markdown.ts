// SPDX-License-Identifier: BSD-2-Clause

import { modeName } from "../catalog.ts";
import { type CodeChange, codeTable } from "../code-change.ts";
import { formatChange, type ReportSummary, type Row, short } from "./report.ts";

interface RenderOptions {
  language: string;
  hash: string;
  runUrl: string;
  baseRef?: string;
  chartRef?: string | null;
  trendRef?: string | null;
  imagesUnavailable?: boolean;
  codeChange?: CodeChange;
}

/** A measured verdict, independent of Cloud and image publication. */
export function headline(summary: ReportSummary) {
  return (
    [
      ...(summary.regressions ? [`${summary.regressions} slower`] : []),
      ...(summary.improvements ? [`${summary.improvements} faster`] : []),
    ].join(", ") || "no regression alert"
  );
}

function cell(text: string) {
  return text.replaceAll("|", "\\|").replace(/[\r\n]/g, " ");
}

const ratio = (value: number | null) => (value === null ? "—" : `${value.toFixed(4)}×`);
const status = { regression: "slower", improvement: "faster", within: "", missing: "new" };

function table(rows: Row[]) {
  return [
    "| Input | Type | N | Mode | Median interval | Current / baseline | Change | Alert | ID |",
    "|---|---|---:|---|---:|---:|---:|---|---|",
    ...rows.map((r) =>
      [
        "",
        cell(r.parts.input),
        cell(r.parts.type),
        r.parts.n,
        cell(modeName(r.parts.mode)),
        `${ratio(r.lower)} – ${ratio(r.upper)}`,
        ratio(r.value),
        formatChange(r.change),
        status[r.verdict],
        `\`${r.name}\``,
        "",
      ].join(" | "),
    ),
  ].join("\n");
}

/** Render one paired report for checks, the PR comment and the job summary. */
export function render(summary: ReportSummary, options: RenderOptions) {
  const { baseRef = "main", chartRef, trendRef, runUrl } = options;
  const changed = summary.rows.filter((row) => row.verdict !== "within");
  const out = [`### Benchmarks · ${options.language} · ${headline(summary)}`];
  const code = options.codeChange;
  if (code) {
    out.push("", codeTable(code, { baseline: baseRef, current: "PR" }));
  } else {
    out.push("", "Core sorting engine production-line analysis unavailable; see the workflow log.");
  }
  if (summary.regressions) {
    out.push("", "> [!CAUTION]", `> **${summary.regressions} cases slower**.`);
    const slower = summary.rows.filter((row) => row.verdict === "regression");
    for (const row of slower.slice(0, 3))
      out.push(`> - ${row.label}: **${formatChange(row.change)}**.`);
    if (slower.length > 3) out.push(`> - and ${slower.length - 3} more below.`);
  }
  if (summary.best)
    out.push(
      "",
      "> [!TIP]",
      "> **" +
        summary.improvements +
        " cases faster**. Best: " +
        summary.best.label +
        ", **" +
        formatChange(summary.best.change) +
        "**.",
    );
  if (chartRef) out.push("", `![Changes against ${baseRef}](${chartRef})`);
  if (trendRef) out.push("", `![PR benchmark history](${trendRef})`);
  if (changed.length)
    out.push(
      "",
      `<details><summary>${changed.length} changed cases</summary>`,
      "",
      table(changed),
      "",
      "</details>",
    );
  out.push(
    "",
    `<details><summary>All ${summary.rows.length} cases</summary>`,
    "",
    table(summary.rows),
    "",
    "</details>",
  );
  const links = [
    `${short(summary.hash)} → \`${baseRef}\` ${short(summary.baseHash)}`,
    `\`${summary.testbed.name}\``,
    `[Run](${runUrl})`,
  ];
  if (summary.project && summary.uuid)
    links.push(
      `[Report](https://bencher.dev/perf/${summary.project}/reports/${summary.uuid})`,
      "[JSON](https://api.bencher.dev/v0/projects/" +
        summary.project +
        "/reports/" +
        summary.uuid +
        ")",
    );
  if (options.imagesUnavailable) links.push(`[Images unavailable](${runUrl})`);
  out.push("", `<sub>${links.join(" · ")}</sub>`);
  return out.join("\n");
}

/** Keep failed measurement evidence explicit without manufacturing a verdict. */
export function renderFailure(options: RenderOptions & { reason: string }) {
  return (
    "### Benchmarks · " +
    options.language +
    " · results unavailable\n\n**" +
    short(options.hash) +
    "** results could not be validated: " +
    options.reason +
    ". See the [workflow run](" +
    options.runUrl +
    ")."
  );
}
