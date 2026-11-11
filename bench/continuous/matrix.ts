// SPDX-License-Identifier: BSD-2-Clause

import { caseLabel, type caseParts, modeOrder } from "../catalog.ts";
import type { ReportSummary, Row } from "./report.ts";

export { modeName } from "../catalog.ts";

/** One column in a comparison figure; hosted reporting currently supplies one. */
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
