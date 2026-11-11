// SPDX-License-Identifier: BSD-2-Clause

import type { caseParts } from "../catalog.ts";

export { caseLabel, caseParts } from "../catalog.ts";

export interface BencherReport {
  uuid: string;
  start_time?: string;
  end_time?: string;
  project: { slug: string };
  testbed: { uuid: string; name: string; spec?: { uuid: string } | null };
  branch: {
    uuid: string;
    name: string;
    head: {
      uuid: string;
      version: { hash: string } | null;
      start_point: {
        branch: string;
        head: string;
        version: { hash: string } | null;
      } | null;
    };
  };
  results: {
    benchmark: { uuid: string; name: string };
    measures: {
      measure: { uuid: string; name: string };
      metric: { uuid: string; value: number };
    }[];
  }[][];
}

interface CaseResult {
  name: string;
  parts: ReturnType<typeof caseParts>;
  label: string;
  benchmark: string;
  measure: string;
  value: number;
  lower: number | null;
  upper: number | null;
}

export interface ComparedRow extends CaseResult {
  verdict: "regression" | "improvement" | "within";
  baseline: number;
  change: number;
}

interface NewRow extends CaseResult {
  verdict: "missing";
  baseline: null;
  change: null;
}

export type Row = ComparedRow | NewRow;
export type Verdict = Row["verdict"];
export type ReportSummary = ReturnType<typeof summarizeRows>;

interface ReportIdentity {
  hash: string | null;
  baseHash: string | null;
  history: { branch: string; head: string }[];
  testbed: BencherReport["testbed"];
  project: string;
  uuid: string;
}

export function short(hash: string | null) {
  return hash ? hash.slice(0, 7) : "unknown";
}

export function formatPercent(v: number) {
  const pct = Math.round(Math.abs(v) * 100);
  return v === 0 ? "0" : `${v > 0 ? "+" : "−"}${pct}%`;
}

export function formatChange(change: number | null) {
  if (change == null || !Number.isFinite(change)) return "—";
  const pct = change * 100;
  return `${pct >= 0 ? "+" : "−"}${Math.abs(pct).toFixed(1)}%`;
}

const VERDICTS = {
  regression: { order: 0 },
  improvement: { order: 1 },
  missing: { order: 2 },
  within: { order: 3 },
};

// Display tier for alerted cases, chosen for readability rather than
// measured: the threshold model alone decides what is alerted, and this only
// decides which changes are set in bold.
export const MAJOR = 0.2;

export function isMajor(r: Row) {
  return Math.abs(r.change ?? 0) >= MAJOR;
}

/** Decode Cloud history values; raw paired samples remain the verdict authority. */
export function cloudRows(report: BencherReport) {
  return (report.results?.[0] ?? []).flatMap((result) =>
    result.measures.map((entry) => {
      const value = entry.metric.value;
      if (!Number.isFinite(value) || value <= 0) throw new Error("Invalid Cloud ratio");
      return {
        name: result.benchmark.name,
        benchmark: result.benchmark.uuid,
        measure: entry.measure.uuid,
        value,
      };
    }),
  );
}

/** Order authoritative row verdicts and summarize each build independently. */
export function summarizeRows(rows: Row[], identity: ReportIdentity) {
  // Worst first: the largest slowdowns, then the largest speedups.
  rows.sort(
    (a, b) =>
      VERDICTS[a.verdict].order - VERDICTS[b.verdict].order ||
      (a.verdict === "improvement"
        ? (a.change ?? 0) - (b.change ?? 0)
        : (b.change ?? 0) - (a.change ?? 0)) ||
      a.name.localeCompare(b.name),
  );
  const count = (verdict: Verdict) => rows.filter((r) => r.verdict === verdict).length;
  // The geometric mean of current/baseline weighs a 2× win on a 700 ns case
  // and on a 10 ms case equally, which an arithmetic mean of times would not.
  const compared = rows.filter((r): r is ComparedRow => r.baseline !== null && r.value > 0);
  const geomean = compared.length
    ? Math.exp(
        compared.reduce((sum, r) => sum + Math.log(r.value / r.baseline), 0) / compared.length,
      )
    : null;
  return {
    rows,
    compared: compared.length,
    geomean,
    best: rows.find((r) => r.verdict === "improvement") ?? null,
    worst: rows.find((r) => r.verdict === "regression") ?? null,
    regressions: count("regression"),
    improvements: count("improvement"),
    missing: count("missing"),
    ...identity,
  };
}

export function secondary(r: Row) {
  return [r.parts.type, r.parts.n].filter(Boolean).join(" · ");
}
