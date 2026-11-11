// SPDX-License-Identifier: BSD-2-Clause

import {
  type BencherReport,
  type ComparedRow,
  type ReportSummary,
  type Row,
  summarize,
} from "./report.ts";

const API = "https://api.bencher.dev/v0/projects";
const PAGE_SIZE = 100;
// The smallest peak-to-trough swing within a PR that earns a trend row. It is
// a display choice, not a statistical test, and is separate from the
// comparison chart's rule: that one judges the final change against the base,
// so a regression later fixed in the same PR disappears there and must stay
// here.
export const MIN_SWING = 0.05;

export interface TrendSeries {
  name: string;
  label: string;
  baseValue: number;
  firstValue: number;
  lastValue: number;
  // Change against the comparison baseline, one per PR run.
  changes: number[];
}

interface Measurement {
  uuid: string;
  hash: string | null;
  time: string;
}

export interface TrendSummary {
  project: string;
  branch: string;
  testbed: string;
  // The base commit the latest report compared against.
  base: string | null;
  sharedCases: number;
  // Current API groups, including groups without a comparable history yet.
  modes: string[];
  runs: Measurement[];
  // Geometric mean of the changes against the base, one per PR run.
  overall: number[];
  focus: TrendSeries[];
  // Every shared case, so a case selected in one configuration can still be
  // drawn beside its quiet history in another.
  series: TrendSeries[];
}

interface MeasuredRun extends Measurement {
  values: Map<string, number>;
}

// Branch heads reset for each PR comparison. Listing by branch retains the
// earlier heads and same-commit reruns that a one-head perf query would miss.
export async function loadHistory(current: BencherReport, request: typeof fetch = fetch) {
  const time = Date.parse(current.start_time ?? "");
  if (!Number.isFinite(time)) throw new Error("The report has no measurement time");
  const reports = new Map<string, BencherReport>();
  for (let page = 1; ; page++) {
    const url = new URL(`${API}/${encodeURIComponent(current.project.slug)}/reports`);
    url.search = new URLSearchParams({
      branch: current.branch.uuid,
      testbed: current.testbed.uuid,
      expand: "true",
      direction: "asc",
      end_time: String(time),
      per_page: String(PAGE_SIZE),
      page: String(page),
    }).toString();
    const response = await request(url, { signal: AbortSignal.timeout(10_000) });
    if (!response.ok) throw new Error(`Bencher history returned HTTP ${response.status}`);
    const batch: BencherReport[] = await response.json();
    if (!Array.isArray(batch)) throw new Error("Bencher history is not a report list");
    const before = reports.size;
    for (const report of batch) reports.set(report.uuid, report);
    if (batch.length < PAGE_SIZE) break;
    if (reports.size === before) throw new Error("Bencher history pagination did not advance");
  }
  // The list can lag a just-completed measurement.
  reports.set(current.uuid, current);
  return [...reports.values()];
}

function caseKey(row: Row) {
  return `${row.benchmark}/${row.measure}`;
}

function comparableRuns(reports: BencherReport[], current: BencherReport, language: string) {
  const cutoff = Date.parse(current.start_time ?? "");
  if (!Number.isFinite(cutoff)) throw new Error("The report has no measurement time");
  const unique = new Map(reports.map((report) => [report.uuid, report]));
  unique.set(current.uuid, current);
  const runs: MeasuredRun[] = [];
  for (const report of unique.values()) {
    if (
      report.branch.uuid !== current.branch.uuid ||
      report.testbed.uuid !== current.testbed.uuid ||
      report.testbed.spec?.uuid !== current.testbed.spec?.uuid
    )
      continue;
    const time = report.start_time;
    if (
      !time ||
      !Number.isFinite(Date.parse(time)) ||
      Date.parse(time) > cutoff ||
      !report.results?.[0]?.length
    )
      continue;
    const rows = summarize(report).rows.filter(
      (row) => row.name.startsWith(`${language}/`) && row.value > 0,
    );
    runs.push({
      uuid: report.uuid,
      hash: report.branch.head.version?.hash ?? null,
      time,
      values: new Map(rows.map((row) => [caseKey(row), row.value])),
    });
  }
  return runs.sort(
    (a, b) =>
      Date.parse(a.time) - Date.parse(b.time) ||
      Number(a.uuid === current.uuid) - Number(b.uuid === current.uuid) ||
      a.uuid.localeCompare(b.uuid),
  );
}

// Every run is compared with the baseline from the latest report, which
// Bencher computes from the start point's own history: `main`, or the parent
// PR of a stacked one. One fixed reference keeps rebases and force pushes out
// of the calculation: an earlier run on an older base still shows how far that
// code was from the base a reviewer would merge into. Cases new in the PR have
// no baseline and stay in the table only. The cohort is also fixed across
// runs, so a case added to or removed from the suite cannot move the overall
// line.
export function summarizeTrend(
  reports: BencherReport[],
  current: BencherReport,
  language: string,
): TrendSummary | null {
  const runs = comparableRuns(reports, current, language);
  if (!runs.length) return null;
  const summary = summarize(current);
  return summarizeRuns(runs, summary, current, language);
}

/** Summarize ratios only within the native Head keyed by protocol and exact baseline.
 *
 * The current point remains available when Cloud or history loading fails.
 * Raw paired verdicts remain authoritative; Cloud supplies history and links.
 */
export function summarizePairedTrend(
  summary: ReportSummary,
  language: string,
  current?: BencherReport,
  reports: BencherReport[] = [],
): TrendSummary | null {
  if (!summary.relative || !summary.baseHash)
    throw new Error("Paired history needs an exact baseline");
  if (!current) {
    return summarizeRuns(
      [
        {
          uuid: summary.hash!,
          hash: summary.hash,
          time: "",
          values: new Map(summary.rows.map((row) => [caseKey(row), row.value])),
        },
      ],
      summary,
      {
        project: { slug: summary.project },
        branch: { name: "" },
        testbed: summary.testbed,
      },
      language,
    );
  }
  const identities = new Map(summarize(current).rows.map((row) => [row.name, row]));
  const rows = summary.rows.map((row) => {
    const identity = identities.get(row.name);
    if (!identity) throw new Error("Cloud history lacks a current case");
    return { ...row, benchmark: identity.benchmark, measure: identity.measure };
  });
  const runs = comparableRuns(
    reports.filter((report) => report.branch.head.uuid === current.branch.head.uuid),
    current,
    language,
  );
  return summarizeRuns(runs, { rows, baseHash: summary.baseHash }, current, language);
}

function summarizeRuns(
  runs: MeasuredRun[],
  summary: Pick<ReportSummary, "rows" | "baseHash">,
  current: { project: { slug: string }; branch: { name: string }; testbed: { name: string } },
  language: string,
): TrendSummary | null {
  const shared = summary.rows.filter(
    (row): row is ComparedRow =>
      row.baseline !== null &&
      row.name.startsWith(`${language}/`) &&
      runs.every((run) => run.values.has(caseKey(row))),
  );
  if (!shared.length) return null;
  // Every shared key is present and positive in every run above.
  const changes = (row: ComparedRow) =>
    runs.map((run) => run.values.get(caseKey(row))! / row.baseline - 1);
  const swings = new Map(
    shared.map((row) => {
      const series = changes(row);
      // A ratio of the PR's own values, so a case far from main does not
      // magnify its rerun noise.
      return [caseKey(row), (1 + Math.max(...series)) / (1 + Math.min(...series)) - 1];
    }),
  );
  // The swing is measured between PR runs, not from main, so it catches a
  // regression fixed later in the PR and leaves steady gaps to main to the
  // comparison chart. Current regressions stay visible in a quiet history.
  // Rows run from the largest swing down, whichever way each case moved.
  const selected = shared
    .filter((row) => row.verdict === "regression" || swings.get(caseKey(row))! >= MIN_SWING)
    .sort(
      (a, b) => swings.get(caseKey(b))! - swings.get(caseKey(a))! || a.name.localeCompare(b.name),
    );
  const series = (row: ComparedRow): TrendSeries => ({
    name: row.name,
    label: row.label,
    baseValue: row.baseline,
    firstValue: runs[0].values.get(caseKey(row))!,
    lastValue: runs.at(-1)!.values.get(caseKey(row))!,
    changes: changes(row),
  });
  return {
    project: current.project.slug,
    branch: current.branch.name,
    testbed: current.testbed.name,
    base: summary.baseHash,
    sharedCases: shared.length,
    modes: [
      ...new Set(
        summary.rows
          .filter((row) => row.name.startsWith(`${language}/`))
          .map((row) => row.parts.mode),
      ),
    ],
    runs: runs.map(({ uuid, hash, time }) => ({ uuid, hash, time })),
    overall: runs.map(
      (run) =>
        Math.exp(
          shared.reduce(
            (sum, row) => sum + Math.log(run.values.get(caseKey(row))! / row.baseline),
            0,
          ) / shared.length,
        ) - 1,
    ),
    focus: selected.map(series),
    series: shared.map(series),
  };
}
