// SPDX-License-Identifier: BSD-2-Clause

import { readdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { algorithm, catalog, comparisonId, parseComparisonId } from "../catalog.ts";
import { type Cohort, comparisonFigures, type Field } from "../report/charts.ts";
import { trackingAlgorithms } from "./algorithms.ts";
import { type ComparisonData, comparisonSchema, parseRuns, type TrackingRun } from "./result.ts";

// Opponents run only the serial entry point, so the comparison reads serial cases.
const SERIAL = "api";
const key = (id: string) => `rust/${id}/${SERIAL}`;

/**
 * Each algorithm's latest run on the hardware of Tessera's latest run.
 *
 * Runs are independent: their ratios are descriptive, not paired verdicts.
 */
export function latestRuns(runs: Map<string, TrackingRun[]>) {
  const latest = (list: TrackingRun[]) => list.toSorted((a, b) => a.time.localeCompare(b.time));
  const tessera = latest(runs.get("tessera") ?? []).at(-1);
  if (!tessera) return null;
  const chosen = new Map<string, TrackingRun>();
  for (const [id, list] of runs) {
    const run = latest(list.filter((entry) => entry.spec === tessera.spec)).at(-1);
    if (run) chosen.set(id, run);
  }
  return { spec: tessera.spec, runs: chosen };
}

/** The figures' view of independent runs: speeds against slice::sort's run, peaks as recorded. */
export function trackingField(runs: Map<string, TrackingRun>): Field {
  const value = (id: string, name: string) => runs.get(name)?.values[key(id)]?.[0] ?? null;
  const ids = new Set(
    [...runs.values()].flatMap((run) =>
      Object.keys(run.values)
        .filter((entry) => entry.startsWith("rust/") && entry.endsWith(`/${SERIAL}`))
        .map((entry) => entry.slice("rust/".length, -`/${SERIAL}`.length)),
    ),
  );
  return {
    // Serial adapters only; a named parallel variant is not an opponent of serial runs.
    algorithms: trackingAlgorithms
      .filter((entry) => runs.has(entry.id) && entry.threads === undefined)
      .map((entry) => ({ name: entry.id, stable: entry.stable })),
    cases: [...ids].map(parseComparisonId),
    speed: (id, name) => {
      const std = value(id, "std-stable"),
        own = value(id, name);
      return std === null || own === null ? null : std / own;
    },
    nsPerKey: (id, name) => {
      const own = value(id, name);
      return own === null ? null : own / parseComparisonId(id).len;
    },
    peak: (id, name) => runs.get(name)?.memory?.[key(id)] ?? null,
  };
}

/**
 * The cohort follows the continuous plan instead of fixed lengths: rankings use the u64
 * length with the most input patterns that both Tessera and slice::sort measured, and
 * memory the largest u64 length with a random input.
 */
export function trackingCohort(field: Field): Cohort | null {
  const measured = (id: string) =>
    field.speed(id, "tessera") !== null && field.nsPerKey(id, "std-stable") !== null;
  const u64 = field.cases.filter((entry) => entry.ty === "u64" && measured(comparisonId(entry)));
  const lengths = Map.groupBy(u64, (entry) => entry.len);
  const [n] = [...lengths].toSorted((a, b) => b[1].length - a[1].length || b[0] - a[0])[0] ?? [];
  if (n === undefined) return null;
  const order = catalog.distributions.map((entry) => entry.id);
  const shapes = lengths
    .get(n)!
    .map((entry) => entry.distribution)
    .toSorted((a, b) => order.indexOf(a) - order.indexOf(b));
  const memoryN = Math.max(
    ...u64.filter((entry) => entry.distribution === "random").map((entry) => entry.len),
  );
  return {
    n,
    lengths: [n],
    shapes,
    shapesLabel: `the ${shapes.length} measured input patterns`,
    memoryN: Number.isFinite(memoryN) ? memoryN : n,
  };
}

/** Build the published comparison from every algorithm's journals, or null before Tessera has one. */
export function comparisonData(runs: Map<string, TrackingRun[]>): ComparisonData | null {
  const latest = latestRuns(runs);
  if (!latest) return null;
  const field = trackingField(latest.runs);
  const cohort = trackingCohort(field);
  if (!cohort) return null;
  // The site renders each chart itself and needs neither the SVG nor the release table.
  const figures = comparisonFigures(field, cohort, undefined, true).map(
    ({ name, title, chart }) => ({ name, title, chart }),
  );
  return comparisonSchema.parse({
    spec: latest.spec,
    runs: [...latest.runs]
      .filter(([id]) => field.algorithms.some((entry) => entry.name === id))
      .map(([id, run]) => ({
        algorithm: id,
        label: algorithm(id).label,
        hash: run.hash,
        time: run.time,
        workflow: run.workflow,
        memory: run.memory !== undefined,
      })),
    overview: figures.find((figure) => figure.name === "overview") ?? null,
    figures,
  });
}

/** Regenerate `comparison.json` at the data root from every algorithm directory present. */
export async function writeComparison(root: string) {
  const present = new Set(await readdir(root));
  const runs = new Map<string, TrackingRun[]>();
  for (const { id } of trackingAlgorithms) {
    if (!present.has(id)) continue;
    const files = (await readdir(join(root, id))).filter((file) => file.endsWith(".jsonl"));
    const list = (
      await Promise.all(
        files.map(async (file) => parseRuns(await readFile(join(root, id, file), "utf8"))),
      )
    ).flat();
    if (list.length) runs.set(id, list);
  }
  const data = comparisonData(runs);
  if (data) await writeFile(join(root, "comparison.json"), JSON.stringify(data) + "\n");
  return data;
}
