// SPDX-License-Identifier: BSD-2-Clause

export interface ThresholdModel {
  test: string;
  min_sample_size: number;
  max_sample_size: number;
  lower_boundary: number;
  upper_boundary: number;
}

export interface Boundary {
  baseline: number;
  lower_limit: number | null;
  upper_limit: number | null;
}

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
      boundary: Boundary | null;
      threshold?: { model: ThresholdModel } | null;
    }[];
  }[][];
  alerts: {
    metric: { uuid: string };
    limit: "upper" | "lower";
    status?: string;
  }[];
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
export type ReportSummary = ReturnType<typeof summarize>;

export function short(hash: string | null) {
  return hash ? hash.slice(0, 7) : "unknown";
}

// Latency is in nanoseconds; three significant digits are enough for triage.
export function formatTime(ns: number | null) {
  if (ns == null || !Number.isFinite(ns)) return "—";
  const [unit, scale] =
    ns >= 1e9 ? ["s", 1e9] : ns >= 1e6 ? ["ms", 1e6] : ns >= 1e3 ? ["µs", 1e3] : ["ns", 1];
  return `${(ns / scale).toPrecision(3)} ${unit}`;
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

// Benchmark IDs are `<language>/<type>/<distribution>/<length>/<seed>/<mode>`,
// as in docs/development/benchmarking.md. Readers get the words; the ID stays
// in link titles and the full table, where it is copied into `cargo bench`.
const DISTRIBUTIONS: Record<string, string> = {
  random: "random",
  sorted: "sorted",
  reverse: "reverse",
  equal: "all equal",
  runs: "runs",
  d16: "16 values",
  d256: "256 values",
  "zipf1.1": "Zipf 1.1",
  zipf2: "Zipf 2",
  sorted95: "95% sorted",
  sorted99: "99% sorted",
  skew50: "50% one key",
  skew99: "99% one key",
  two_values: "two values",
  sparse8: "8 wide keys",
  sorted_tail: "sorted + tail",
};

export function caseParts(id: string) {
  const parts = id.split("/");
  if (parts.length !== 6 || !/^\d+$/.test(parts[3]))
    return { type: "", input: id, n: "", extra: "", seed: "", mode: "" };
  const [, type, distribution, length, seed, mode] = parts;
  const row = type.match(/^row_(\w+)$/);
  // Seed 1 and the public API are the defaults; only departures are named.
  const extra = [];
  if (seed !== "s1") extra.push(`seed ${seed.slice(1)}`);
  if (mode !== "api") extra.push(mode);
  return {
    type: row ? `{${row[1]},u32}` : type,
    input: DISTRIBUTIONS[distribution] ?? distribution,
    n: Number(length).toLocaleString("en-US"),
    extra: extra.join(", "),
    seed,
    mode,
  };
}

export function caseLabel(id: string) {
  const p = caseParts(id);
  return [p.type, p.input, p.n].filter(Boolean).join(" · ") + (p.extra ? `, ${p.extra}` : "");
}

// One row per benchmark and measure. Alerts are authoritative for the
// verdict; boundaries only supply the numbers shown beside it.
export function summarize(report: BencherReport) {
  const alerts = new Map();
  for (const alert of report.alerts ?? []) {
    if (alert.status && alert.status !== "active") continue;
    // An alert names the metric it fired on; that UUID is unique per result.
    alerts.set(alert.metric.uuid, alert.limit);
  }
  const results = report.results?.[0] ?? [];
  const measureCount = new Set(results.flatMap((r) => r.measures.map((m) => m.measure.uuid))).size;
  const rows: Row[] = [];
  for (const result of results) {
    for (const m of result.measures) {
      const value = m.metric?.value;
      const boundary = m.boundary ?? null;
      const baseline = boundary?.baseline ?? null;
      const limit = alerts.get(m.metric?.uuid);
      const verdict =
        limit === "upper" ? "regression" : limit === "lower" ? "improvement" : "within";
      const name = result.benchmark.name;
      if (
        !Number.isFinite(value) ||
        value < 0 ||
        (baseline !== null && (!Number.isFinite(baseline) || baseline <= 0))
      ) {
        throw new Error(`Invalid latency or baseline for ${name}`);
      }
      if (limit && baseline === null) {
        throw new Error(`Alert has no baseline for ${name}`);
      }
      rows.push({
        name,
        parts: caseParts(name),
        label: caseLabel(name) + (measureCount > 1 ? ` (${m.measure.name})` : ""),
        benchmark: result.benchmark.uuid,
        measure: m.measure.uuid,
        value,
        lower: boundary?.lower_limit ?? null,
        upper: boundary?.upper_limit ?? null,
        ...(baseline === null
          ? { baseline: null, change: null, verdict: "missing" as const }
          : { baseline, change: value / baseline - 1, verdict }),
      });
    }
  }
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
  const head = report.branch.head;
  const history = [{ branch: report.branch.uuid, head: head.uuid }];
  if (head.start_point) {
    history.unshift({ branch: head.start_point.branch, head: head.start_point.head });
  }
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
    hash: head.version?.hash ?? null,
    baseHash: head.start_point?.version?.hash ?? null,
    history,
    testbed: report.testbed,
    project: report.project?.slug,
    uuid: report.uuid,
  };
}

export function secondary(r: Row) {
  return [r.parts.type, r.parts.n].filter(Boolean).join(" · ");
}
