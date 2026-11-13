// SPDX-License-Identifier: BSD-2-Clause

import { readdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { algorithm, catalog, comparisonId, distribution, parseComparisonId } from "../catalog.ts";
import {
  type Cohort,
  comparisonFigures,
  type Field,
  renderParallel,
  type Scaling,
} from "../report/charts.ts";
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

// The ranking compares 32-bit-key records: the one format every radix opponent supports,
// including the original Block LSD implementation of the paper Tessera's partition adapts.
const RANKING_TYPE = "row_u32";

/**
 * The cohort follows the continuous plan instead of fixed lengths. The ranking and the
 * u64 input-pattern figures each use their type's length with the most input patterns that
 * both Tessera and slice::sort measured, the longest on a tie; memory uses the longest u64
 * length with a random input.
 */
export function trackingCohort(field: Field): Cohort | null {
  const measured = (id: string) =>
    field.speed(id, "tessera") !== null && field.nsPerKey(id, "std-stable") !== null;
  const order = catalog.distributions.map((entry) => entry.id);
  const widest = (ty: string) => {
    const cases = field.cases.filter((entry) => entry.ty === ty && measured(comparisonId(entry)));
    const [found] = [...Map.groupBy(cases, (entry) => entry.len)].toSorted(
      (a, b) => b[1].length - a[1].length || b[0] - a[0],
    );
    if (!found) return null;
    const shapes = found[1]
      .map((entry) => entry.distribution)
      .toSorted((a, b) => order.indexOf(a) - order.indexOf(b));
    return { n: found[0], shapes, cases };
  };
  const over = (shapes: string[], patterns: string) =>
    shapes.length === 1
      ? `on the ${distribution(shapes[0]).label} input`
      : `geometric mean over the ${shapes.length} ${patterns}`;
  const u64 = widest("u64");
  if (!u64) return null;
  const records = widest(RANKING_TYPE);
  const ranking = records ? { ty: RANKING_TYPE, ...records } : { ty: "u64", ...u64 };
  const { n, shapes } = u64;
  // Element types compare over the patterns every measured type shares at that length.
  const types = Map.groupBy(
    field.cases.filter((entry) => entry.len === n && measured(comparisonId(entry))),
    (entry) => entry.ty,
  );
  const typeShapes = shapes.filter((shape) =>
    [...types.values()].every((cases) => cases.some((entry) => entry.distribution === shape)),
  );
  const memoryN = Math.max(
    ...u64.cases.filter((entry) => entry.distribution === "random").map((entry) => entry.len),
  );
  return {
    ranking: {
      ty: ranking.ty,
      n: ranking.n,
      lengths: [ranking.n],
      shapes: ranking.shapes,
      label: over(ranking.shapes, "measured input patterns"),
    },
    n,
    shapes,
    typeShapes,
    typeShapesLabel: over(typeShapes, "input patterns every type has"),
    memoryN: Number.isFinite(memoryN) ? memoryN : n,
  };
}

/**
 * Tessera's and Rayon's stable parallel variants at every thread count both ran, on each
 * input every one of them measured at their most common length. Rayon on one thread is the
 * baseline: slice::sort has no runs at most parallel lengths.
 */
export function trackingScaling(runs: Map<string, TrackingRun>): Scaling | null {
  const variant = (adapter: string, threads: number) =>
    trackingAlgorithms.find((entry) => entry.adapter === adapter && entry.threads === threads);
  const counts = trackingAlgorithms
    .filter((entry) => entry.adapter === "tessera-par" && entry.threads !== undefined)
    .map((entry) => entry.threads!)
    .filter((t) =>
      [variant("tessera-par", t), variant("rayon-stable", t)].every((e) => e && runs.has(e.id)),
    )
    .toSorted((a, b) => a - b);
  if (counts[0] !== 1) return null;
  const time = (adapter: string, t: number, id: string) =>
    runs.get(variant(adapter, t)!.id)!.values[`rust/${id}/par-${t}`]?.[0];
  const ids = Object.keys(runs.get(variant("rayon-stable", 1)!.id)!.values)
    .map((key) => key.slice("rust/".length, key.lastIndexOf("/")))
    .filter((id) => counts.every((t) => time("tessera-par", t, id) && time("rayon-stable", t, id)));
  const [found] = [...Map.groupBy(ids, (id) => parseComparisonId(id).len)].toSorted(
    (a, b) => b[1].length - a[1].length || b[0] - a[0],
  );
  if (!found) return null;
  const [n, inputs] = found;
  const speed = (adapter: string, t: number, id: string) =>
    time("rayon-stable", 1, id)! / time(adapter, t, id)!;
  const of = (adapter: string, id: string) => counts.map((t) => speed(adapter, t, id));
  // Tessera's parallel journals update only when dispatched, so they name their own commits.
  const hashes = [
    ...new Set(counts.map((t) => runs.get(variant("tessera-par", t)!.id)!.hash.slice(0, 7))),
  ];
  return {
    n,
    threads: counts,
    tessera: hashes,
    inputs: inputs.map((id) => {
      const { ty, distribution: shape } = parseComparisonId(id);
      return {
        label: `${ty} · ${distribution(shape).label}`,
        tessera: of("tessera-par", id),
        rayon: of("rayon-stable", id),
      };
    }),
  };
}

/**
 * Build the published comparison from every algorithm's journals. Without a cohort, such as
 * before `slice::sort` has a run on Tessera's latest hardware, it has no figures, so the site
 * never shows an earlier comparison as the latest one.
 */
export function comparisonData(runs: Map<string, TrackingRun[]>): ComparisonData {
  const latest = latestRuns(runs);
  const field = trackingField(latest?.runs ?? new Map());
  const cohort = trackingCohort(field);
  // The site renders each chart itself; the table names every plotted value.
  const scaling = latest && cohort ? trackingScaling(latest.runs) : null;
  const parallel = scaling ? renderParallel(scaling) : null;
  // Parallel sorting follows the serial figures and precedes memory.
  const all = cohort ? comparisonFigures(field, cohort, undefined, true) : [];
  const memory = all.findIndex((figure) => figure.name === "memory");
  if (parallel) all.splice(memory < 0 ? all.length : memory, 0, parallel);
  const figures = all.map(({ name, subject, summary, title, chart, table }) => ({
    name,
    subject,
    summary,
    title,
    chart,
    table,
  }));
  return comparisonSchema.parse({
    spec: latest?.spec ?? null,
    runs: [...(latest?.runs ?? [])]
      .filter(([id]) => cohort && field.algorithms.some((entry) => entry.name === id))
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
  await writeFile(join(root, "comparison.json"), `${JSON.stringify(data)}\n`);
  return data;
}
