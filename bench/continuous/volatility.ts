// SPDX-License-Identifier: BSD-2-Clause

import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { VARIANTS } from "../catalog.ts";
import { median } from "../statistics.ts";
import { loadHistory } from "./history.ts";
import { type BencherReport, summarize } from "./report.ts";

/** Describe adjacent main samples within one Testbed/spec, retaining same-commit reruns. */
export function summarizeVolatility(reports: BencherReport[], current: BencherReport) {
  const cutoff = Date.parse(current.start_time ?? "");
  if (current.branch.name !== "main" || !Number.isFinite(cutoff))
    throw new Error("Expected a dated main report");
  const runs = [...new Map(reports.map((report) => [report.uuid, report])).values()]
    .filter(
      (report) =>
        report.branch.uuid === current.branch.uuid &&
        report.branch.name === "main" &&
        report.testbed.uuid === current.testbed.uuid &&
        report.testbed.spec?.uuid === current.testbed.spec?.uuid &&
        Date.parse(report.start_time ?? "") <= cutoff &&
        !!report.end_time &&
        !!report.results?.[0]?.length,
    )
    .sort(
      (a, b) =>
        Date.parse(a.start_time!) - Date.parse(b.start_time!) || a.uuid.localeCompare(b.uuid),
    );
  const cases = new Map<
    string,
    { id: string; measure: string; changes: number[]; sameCommitChanges: number[] }
  >();
  for (let index = 1; index < runs.length; index++) {
    const previous = new Map(
      summarize(runs[index - 1]).rows.map((row) => [`${row.benchmark}/${row.measure}`, row]),
    );
    for (const row of summarize(runs[index]).rows) {
      const key = `${row.benchmark}/${row.measure}`;
      const base = previous.get(key);
      if (
        !row.name.startsWith("rust/") ||
        !base ||
        !Number.isFinite(row.value) ||
        !Number.isFinite(base.value) ||
        row.value <= 0 ||
        base.value <= 0
      )
        continue;
      let entry = cases.get(key);
      if (!entry) {
        entry = { id: row.name, measure: row.measure, changes: [], sameCommitChanges: [] };
        cases.set(key, entry);
      }
      const change = row.value / base.value - 1;
      entry.changes.push(change);
      const hash = runs[index].branch.head.version?.hash;
      if (hash && hash === runs[index - 1].branch.head.version?.hash)
        entry.sameCommitChanges.push(change);
    }
  }
  return {
    testbed: current.testbed,
    cutoff: current.start_time,
    runs: runs.map((run) => ({
      uuid: run.uuid,
      hash: run.branch.head.version?.hash,
      time: run.start_time,
    })),
    cases: [...cases.values()]
      .map((row) => ({
        ...row,
        pairs: row.changes.length,
        medianAbsoluteChange: median(row.changes.map(Math.abs)),
        maxAbsoluteChange: Math.max(...row.changes.map(Math.abs)),
        outsideFivePercent: row.changes.filter((change) => 1 + change < 0.95 || 1 + change > 1.05)
          .length,
      }))
      .sort((a, b) => b.maxAbsoluteChange - a.maxAbsoluteChange || a.id.localeCompare(b.id)),
  };
}

interface MainHistory {
  schema: 1;
  kind: "main-adjacent-history";
  project: string;
  retrieved: string;
  configurations: { current: BencherReport; reports: BencherReport[] }[];
}

/** Regenerate a read-only history report from retained API responses. */
export async function reportMainHistory(out: string, history?: MainHistory) {
  history ??= JSON.parse(await readFile(join(out, "history.json"), "utf8"));
  if (
    history?.schema !== 1 ||
    history.kind !== "main-adjacent-history" ||
    !history.configurations.length
  )
    throw new Error("Incompatible main history");
  const lines = [
    "# Adjacent main measurement changes",
    "",
    `Project: ${history.project}. Retrieved: ${history.retrieved}.`,
    "",
    "Each configuration is filtered by branch, Testbed and hardware spec. Adjacent completed reports are paired only where the same benchmark and measure are present. Source changes, placement and host variation are mixed. These observations do not estimate layout-only noise and must not automatically widen thresholds. Same-commit pairs are retained separately in summaries.json.",
    "",
  ];
  const summaries = history.configurations.map(({ current, reports }) =>
    summarizeVolatility(reports, current),
  );
  for (const summary of summaries) {
    lines.push(
      `## ${summary.testbed.name}`,
      "",
      `${summary.runs.length} completed reports through ${summary.cutoff}; ${summary.cases.length} cases with adjacent observations.`,
      "",
      "| Case | Pairs | Median absolute change | Maximum absolute change | Outside ±5% | Same-commit pairs |",
      "|---|---:|---:|---:|---:|---:|",
    );
    for (const row of summary.cases)
      lines.push(
        `| ${row.id} | ${row.pairs} | ${(row.medianAbsoluteChange * 100).toFixed(2)}% | ${(row.maxAbsoluteChange * 100).toFixed(2)}% | ${row.outsideFivePercent} | ${row.sameCommitChanges.length} |`,
      );
    lines.push("");
  }
  await writeFile(join(out, "summaries.json"), `${JSON.stringify(summaries, null, 2)}\n`);
  await writeFile(join(out, "index.md"), lines.join("\n"));
}

/** Read both catalogued main histories from the public API, without modifying Bencher. */
export async function readMainHistory(project: string, out: string, request: typeof fetch = fetch) {
  const configurations: MainHistory["configurations"] = [];
  for (const variant of VARIANTS) {
    const url = new URL(
      `https://api.bencher.dev/v0/projects/${encodeURIComponent(project)}/reports`,
    );
    url.search = new URLSearchParams({
      branch: "main",
      testbed: variant.testbed,
      direction: "desc",
      per_page: "1",
      expand: "true",
    }).toString();
    const response = await request(url, { signal: AbortSignal.timeout(10_000) });
    if (!response.ok) throw new Error(`Bencher main returned HTTP ${response.status}`);
    const reports: BencherReport[] = await response.json();
    const current = reports[0];
    if (
      !Array.isArray(reports) ||
      !current ||
      current.branch.name !== "main" ||
      current.testbed.name !== variant.testbed
    )
      throw new Error(`No matching main report for ${variant.testbed}`);
    configurations.push({ current, reports: await loadHistory(current, request) });
  }
  const history: MainHistory = {
    schema: 1,
    kind: "main-adjacent-history",
    project,
    retrieved: new Date().toISOString(),
    configurations,
  };
  await mkdir(out, { recursive: false });
  await writeFile(join(out, "history.json"), `${JSON.stringify(history, null, 2)}\n`);
  await reportMainHistory(out, history);
}
