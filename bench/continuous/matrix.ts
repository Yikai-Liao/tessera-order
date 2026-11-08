// SPDX-License-Identifier: BSD-2-Clause

import { caseLabel, type caseParts, modeOrder } from "../catalog.ts";
import type { ReportSummary, Row } from "./report.ts";

export { modeName } from "../catalog.ts";

// One build configuration's measurement. A configuration is a separately
// compiled executable with its own Testbed and baseline, such as Portable
// SIMD. API variants of one executable (stable, later unstable or
// parallel sorts) are not configurations: they are cases, named by the last
// segment of the benchmark ID, so they add rows rather than columns.
export interface Configuration {
  name: string;
  summary: ReportSummary | null;
}

// A case across every configuration. Cells follow the configurations' order;
// a cell is absent when that configuration failed or lacks the case.
export interface CaseRow {
  name: string;
  parts: ReturnType<typeof caseParts>;
  label: string;
  cells: (Row | undefined)[];
}

export function caseRows(configurations: Configuration[]): CaseRow[] {
  const rows = new Map<string, CaseRow>();
  configurations.forEach((configuration, i) => {
    for (const row of configuration.summary?.rows ?? []) {
      let entry = rows.get(row.name);
      if (!entry) {
        entry = {
          name: row.name,
          parts: row.parts,
          label: caseLabel(row.name),
          cells: configurations.map(() => undefined),
        };
        rows.set(row.name, entry);
      }
      entry.cells[i] = row;
    }
  });
  return [...rows.values()];
}

const alerted = (row: CaseRow, verdict: Row["verdict"]) =>
  row.cells.filter((cell) => cell?.verdict === verdict) as Row[];

export function isSlower(row: CaseRow) {
  return alerted(row, "regression").length > 0;
}

export function isFaster(row: CaseRow) {
  return !isSlower(row) && alerted(row, "improvement").length > 0;
}

// The change that decides a row's place: its largest alerted slowdown, or for
// a faster-only row its largest alerted speedup.
export function severity(row: CaseRow) {
  const slower = alerted(row, "regression").map((cell) => cell.change!);
  if (slower.length) return Math.max(...slower);
  const faster = alerted(row, "improvement").map((cell) => cell.change!);
  return faster.length ? Math.min(...faster) : 0;
}

// Cases with an alert in any configuration, grouped by mode; within a mode,
// slower cases first from the worst, then faster ones from the best.
export function changedRows(configurations: Configuration[]) {
  return caseRows(configurations)
    .filter((row) => isSlower(row) || isFaster(row))
    .sort(
      (a, b) =>
        modeOrder(a.parts.mode) - modeOrder(b.parts.mode) ||
        a.parts.mode.localeCompare(b.parts.mode) ||
        Number(isFaster(a)) - Number(isFaster(b)) ||
        (isSlower(a) ? severity(b) - severity(a) : severity(a) - severity(b)) ||
        a.name.localeCompare(b.name),
    );
}

// Which configurations a set of alerts touched, in words: "both", "all", or
// the names. Whether a change is shared or confined to one build is the
// first clue to its cause, so the report states it instead of leaving the
// reader to compare columns.
export function attribution(configurations: Configuration[], hit: boolean[]) {
  const names = configurations.filter((_, i) => hit[i]).map((c) => c.name);
  if (configurations.length === 1) return null;
  if (names.length === configurations.length) return configurations.length === 2 ? "both" : "all";
  return `${names.join(", ")} only`;
}

// The geometric mean of current/baseline over a configuration's compared
// cases, optionally within one mode.
export function geomean(summary: ReportSummary, mode?: string) {
  const compared = summary.rows.filter(
    (r) => r.baseline !== null && r.value > 0 && (mode === undefined || r.parts.mode === mode),
  );
  if (!compared.length) return null;
  return {
    change:
      Math.exp(
        compared.reduce((sum, r) => sum + Math.log(r.value / r.baseline!), 0) / compared.length,
      ) - 1,
    cases: compared.length,
  };
}

export function modes(configurations: Configuration[]) {
  const all = new Set(
    configurations.flatMap((c) => c.summary?.rows.map((r) => r.parts.mode) ?? []),
  );
  return [...all].sort((a, b) => modeOrder(a) - modeOrder(b) || a.localeCompare(b));
}
