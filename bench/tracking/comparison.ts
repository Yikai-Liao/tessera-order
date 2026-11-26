// SPDX-License-Identifier: BSD-2-Clause

import { readdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import {
  type CaseIdentity,
  caseId,
  catalog,
  columnId,
  criterionId,
  distribution,
  type Execution,
  executionThreads,
  isComposition,
  type Operation,
  operationDescription,
  operationKey,
  operationKinds,
  operationParts,
  parseCaseId,
  parseColumnId,
  parseCriterionId,
  referenceColumn,
  referenceLabel,
  routeLabel,
  routeStable,
  SORT,
} from "../catalog.ts";
import {
  type Cohort,
  comparisonFigures,
  type Field,
  type Growth,
  parameterFigures,
  renderParallel,
  type Scaling,
} from "../report/charts.ts";
import { type GrowthRecord, growthSchema } from "./growth.ts";
import { trackingImplementations } from "./implementations.ts";
import { type ComparisonData, comparisonSchema, parseRuns, type TrackingRun } from "./result.ts";

/** A column's value on a case, keyed as Criterion names it. */
const timed = (run: TrackingRun | undefined, id: string, execution: Execution) =>
  run?.values[criterionId(parseCaseId(id), execution)];

/**
 * Each column's latest run on the hardware of Tessera's latest serial run, by column ID.
 *
 * Runs are independent: their ratios are descriptive, not paired verdicts.
 */
export function latestRuns(runs: Map<string, TrackingRun[]>) {
  const byTime = (list: TrackingRun[]) => list.toSorted((a, b) => a.time.localeCompare(b.time));
  const tessera = byTime(
    (runs.get("tessera") ?? []).filter((entry) => entry.execution === "serial"),
  ).at(-1);
  if (!tessera) return null;
  const chosen = new Map<string, TrackingRun>();
  for (const [id, list] of runs)
    for (const run of byTime(list.filter((entry) => entry.spec === tessera.spec)))
      chosen.set(columnId({ implementation: id, execution: run.execution as Execution }), run);
  return { spec: tessera.spec, runs: chosen };
}

/** An operation's serial reference implementation. */
const serialReference = (operation: Operation) =>
  referenceColumn(operation, "serial")!.implementation;

/**
 * The figures' views of one operation in serial columns. The main figures use one call with
 * fixed parameters, chosen by shared coverage and then lowest parameter values. The parameter
 * figures have every case Tessera and the operation's reference both measured.
 */
function trackingFields(runs: Map<string, TrackingRun>, kind: Operation["kind"]) {
  const serial = (name: string) =>
    runs.get(columnId({ implementation: name, execution: "serial" }));
  const value = (id: string, name: string) => timed(serial(name), id, "serial")?.[0] ?? null;
  const cases = new Map<string, CaseIdentity>();
  for (const [column, run] of runs) {
    if (parseColumnId(column).execution !== "serial") continue;
    for (const key of Object.keys(run.values)) {
      const input = parseCriterionId(key);
      if (input.operation.kind === kind) cases.set(caseId(input), input);
    }
  }
  const measured = [...cases.values()].filter(
    (input) =>
      value(caseId(input), "tessera") !== null &&
      value(caseId(input), serialReference(input.operation)) !== null,
  );
  // Break coverage ties by parameter values in catalog identity order.
  const lexical = (a: number[], b: number[]) => {
    const at = a.findIndex((value, index) => value !== b[index]);
    return at < 0 ? 0 : a[at] - b[at];
  };
  const [chosen] = [...Map.groupBy(measured, (input) => operationKey(input.operation)).values()]
    .toSorted(
      (a, b) =>
        b.length - a.length ||
        lexical(operationParts(a[0].operation).values, operationParts(b[0].operation).values),
    )
    .map((group) => group[0].operation);
  if (!chosen) return null;
  const key = operationKey(chosen);
  const reference = serialReference(chosen);
  const field = (cases: CaseIdentity[]): Field => ({
    operation: chosen,
    // An implementation without this operation's route has no cases and no row.
    implementations: trackingImplementations
      .filter((entry) => kind !== SORT.kind || !isComposition(entry.id))
      .filter((entry) => cases.some((input) => value(caseId(input), entry.id) !== null))
      .map((entry) => ({ name: entry.id, stable: routeStable(entry.id, chosen) })),
    cases,
    speed: (id, name) => {
      const base = value(id, reference),
        own = value(id, name);
      return base === null || own === null ? null : base / own;
    },
    nsPerKey: (id, name) => {
      const own = value(id, name);
      return own === null ? null : own / parseCaseId(id).len;
    },
    peak: (id, name) => serial(name)?.memory?.[criterionId(parseCaseId(id), "serial")] ?? null,
  });
  return {
    primary: field([...cases.values()].filter((input) => operationKey(input.operation) === key)),
    parameters: operationParts(chosen).values.length ? field(measured) : null,
  };
}

// The ranking compares 32-bit-key records: the one format every radix opponent supports,
// including the original Block LSD implementation of the paper Tessera's partition adapts.
const RANKING_TYPE = "row_u32";

/**
 * The cohort follows the continuous plan instead of fixed lengths. The ranking and the
 * input-pattern figures each use their type's length with the most input patterns that
 * both Tessera and the serial reference measured, the longest on a tie. The length and short-input
 * figures use comparison lengths: those where some type has several patterns. Lengths
 * with one pattern per type probe engine thresholds and stay in PR feedback.
 */
export function trackingCohort(field: Field): Cohort | null {
  const measured = (id: string) =>
    field.speed(id, "tessera") !== null &&
    field.nsPerKey(id, serialReference(field.operation)) !== null;
  const order = catalog.distributions.map((entry) => entry.id);
  const widest = (ty: string) => {
    const cases = field.cases.filter((entry) => entry.ty === ty && measured(caseId(entry)));
    const [found] = [...Map.groupBy(cases, (entry) => entry.len)].toSorted(
      (a, b) => b[1].length - a[1].length || b[0] - a[0],
    );
    if (!found) return null;
    const distributions = found[1]
      .map((entry) => entry.distribution)
      .toSorted((a, b) => order.indexOf(a) - order.indexOf(b));
    return { n: found[0], distributions, cases };
  };
  const over = (distributions: string[], patterns: string) =>
    distributions.length === 1
      ? `on the ${distribution(distributions[0]).label} input`
      : `geometric mean over the ${distributions.length} ${patterns}`;
  const u64 = widest("u64");
  if (!u64) return null;
  const records = widest(RANKING_TYPE);
  const ranking = records ? { ty: RANKING_TYPE, ...records } : { ty: "u64", ...u64 };
  const { n, distributions } = u64;
  // Element types compare over the patterns every measured type shares at that length.
  const types = Map.groupBy(
    field.cases.filter((entry) => entry.len === n && measured(caseId(entry))),
    (entry) => entry.ty,
  );
  const typeDistributions = distributions.filter((distributionId) =>
    [...types.values()].every((cases) =>
      cases.some((entry) => entry.distribution === distributionId),
    ),
  );
  const shared = field.cases.filter((entry) => measured(caseId(entry)));
  const lengths = [...Map.groupBy(shared, (entry) => entry.len)]
    .filter(([, cases]) =>
      [...Map.groupBy(cases, (entry) => entry.ty).values()].some((of) => of.length > 1),
    )
    .map(([len]) => len)
    .toSorted((a, b) => a - b);
  // Short inputs show every pattern of each type with several patterns at a shorter length.
  const shortLengths = lengths.filter((len) => len < n);
  const shortTypes = catalog.types
    .map((type) => type.id)
    .filter((ty) =>
      shortLengths.some(
        (len) => shared.filter((entry) => entry.ty === ty && entry.len === len).length > 1,
      ),
    );
  return {
    ranking: {
      ty: ranking.ty,
      n: ranking.n,
      lengths: [ranking.n],
      distributions: ranking.distributions,
      label: over(ranking.distributions, "measured input patterns"),
    },
    n,
    sweep: { distributionId: "random", lengths },
    short: { lengths: shortLengths, types: shortTypes },
    distributions,
    typeDistributions,
    typeDistributionsLabel: over(typeDistributions, "input patterns every type has"),
  };
}

/**
 * Tessera's and the parallel reference's columns at every pool size both ran, on each input
 * every one of them measured at their most common length. The reference in one worker is
 * the baseline: the serial reference has no runs at most parallel lengths.
 */
export function trackingScaling(
  runs: Map<string, TrackingRun>,
  operation: Operation = SORT,
): Scaling | null {
  const referenceId = referenceColumn(operation, "t1")?.implementation;
  if (!referenceId) return null;
  const run = (name: string, t: number) =>
    runs.get(columnId({ implementation: name, execution: `t${t}` }));
  const columns = [...runs.keys()].map(parseColumnId);
  const hasOperation = (entry: TrackingRun | undefined) =>
    entry !== undefined &&
    Object.keys(entry.values).some((id) => parseCriterionId(id).operation.kind === operation.kind);
  const counts = columns
    .filter((column) => column.implementation === "tessera")
    .map((column) => executionThreads(column.execution))
    .filter(
      (t): t is number =>
        t !== undefined && hasOperation(run("tessera", t)) && hasOperation(run(referenceId, t)),
    )
    .toSorted((a, b) => a - b);
  if (counts[0] !== 1) return null;
  const time = (name: string, t: number, id: string) => timed(run(name, t), id, `t${t}`)?.[0];
  const ids = Object.keys(run(referenceId, 1)!.values)
    .map(parseCriterionId)
    .filter((input) => input.operation.kind === operation.kind)
    .map(caseId)
    .filter((id) => counts.every((t) => time("tessera", t, id) && time(referenceId, t, id)));
  const [found] = [...Map.groupBy(ids, (id) => parseCaseId(id).len)].toSorted(
    (a, b) => b[1].length - a[1].length || b[0] - a[0],
  );
  if (!found) return null;
  const [n, inputs] = found;
  // Parallel sorting uses the `tessera-full` serial column (which covers every parallel sort input);
  // parallel argsort compares against Tessera's own serial argsort column.
  const compositionColumn =
    operation.kind === SORT.kind
      ? columns.find(
          (column) => column.execution === "serial" && isComposition(column.implementation),
        )
      : columns.find(
          (column) => column.execution === "serial" && column.implementation === "tessera",
        );
  const serial = compositionColumn && runs.get(columnId(compositionColumn));
  const composition = inputs.map((id) => {
    const ns = timed(serial, id, "serial")?.[0];
    return ns === undefined ? null : time(referenceId, 1, id)! / ns;
  });
  const speed = (name: string, t: number, id: string) =>
    time(referenceId, 1, id)! / time(name, t, id)!;
  const of = (name: string, id: string) => counts.map((t) => speed(name, t, id));
  // Pool sizes outside the continuous plan update only when dispatched, so they name their commits.
  const hashes = [...new Set(counts.map((t) => run("tessera", t)!.hash.slice(0, 7)))];
  return {
    operation,
    n,
    threads: counts,
    referenceId,
    tessera: hashes,
    ...(compositionColumn && serial && composition.every((speed) => speed !== null)
      ? {
          composition: {
            implementation: compositionColumn.implementation,
            hash: serial.hash.slice(0, 7),
            speeds: composition,
          },
        }
      : {}),
    inputs: inputs.map((id) => {
      const { ty, distribution: distributionId } = parseCaseId(id);
      return {
        label: `${ty} · ${distribution(distributionId).label}`,
        tessera: of("tessera", id),
        rayon: of(referenceId, id),
      };
    }),
  };
}

/** The growth sweep in the figures' terms: implementations with a serial entry, in catalog order. */
function trackingGrowth(sweep: GrowthRecord["sweeps"][number], hash: string): Growth {
  return {
    operation: sweep.operation,
    ty: sweep.type,
    distribution: sweep.distribution,
    lengths: sweep.lengths,
    peaks: trackingImplementations
      .filter((entry) => entry.parallel !== "only" && Object.hasOwn(sweep.peaks, entry.id))
      .map((entry) => ({ name: entry.id, values: sweep.peaks[entry.id] })),
    source: `Tessera at ${hash.slice(0, 7)}`,
  };
}

/**
 * Build the published comparison from every implementation's journals and the growth sweep:
 * one section per operation with figures, in catalog order. An operation has no section
 * before its reference has a run on Tessera's latest hardware, so the site never shows an
 * earlier comparison as the latest one.
 */
export function comparisonData(
  runs: Map<string, TrackingRun[]>,
  growth?: GrowthRecord,
): ComparisonData {
  const latest = latestRuns(runs);
  const operations = operationKinds.flatMap((kind) => {
    const fields = latest && trackingFields(latest.runs, kind);
    if (!fields) return [];
    const { primary: field, parameters } = fields;
    const axes = parameters ? parameterFigures(parameters) : [];
    const cohort = trackingCohort(field);
    if (!cohort && !axes.length) return [];
    const scaling = trackingScaling(latest.runs, field.operation);
    const parallel = scaling ? renderParallel(scaling) : null;
    const sweep = growth?.sweeps.find((entry) => entry.operation.kind === kind);
    // Without a sweep, the memory figure falls back to the runs' own peaks.
    const growthFigure = growth && sweep ? trackingGrowth(sweep, growth.hash) : undefined;
    // The site renders each chart itself; the table names every plotted value.
    const all = cohort ? comparisonFigures(field, cohort, true, growthFigure) : [];
    const headline = all.find((figure) => figure.name === "overview")?.headline;
    // Parameter axes and parallel scaling follow the fixed-parameter speed figures.
    const memory = all.findIndex((figure) => figure.name === "memory");
    all.splice(memory < 0 ? all.length : memory, 0, ...axes, ...(parallel ? [parallel] : []));
    const names = new Set(field.implementations.map(({ name }) => name));
    if (axes.length && parameters)
      for (const { name } of parameters.implementations) names.add(name);
    return [
      {
        id: kind,
        label: operationDescription(field.operation),
        reference: referenceLabel(field.operation, "serial"),
        ...(headline && { headline }),
        runs: trackingImplementations
          .filter(({ id }) => names.has(id))
          .map(({ id: name }) => {
            const run = latest.runs.get(columnId({ implementation: name, execution: "serial" }))!;
            return {
              implementation: name,
              label: routeLabel(name, field.operation),
              hash: run.hash,
              time: run.time,
              workflow: run.workflow,
              memory: run.memory !== undefined,
            };
          }),
        figures: all.map(({ name, subject, summary, title, kind, chart, table, selection }) => ({
          name,
          subject,
          summary,
          title,
          ...(kind ? { kind } : {}),
          ...(selection ? { selection } : {}),
          chart,
          table,
        })),
      },
    ];
  });
  return comparisonSchema.parse({
    spec: latest?.spec ?? null,
    overview:
      operations
        .find((section) => section.id === SORT.kind)
        ?.figures.find((figure) => figure.name === "overview") ?? null,
    operations,
  });
}

/** The growth sweep's file at the data root, written by `publish.ts growth`. */
export const GROWTH_FILE = "growth.json";

/** Regenerate `comparison.json` at the data root from every implementation directory present. */
export async function writeComparison(root: string) {
  const present = new Set(await readdir(root));
  const runs = new Map<string, TrackingRun[]>();
  for (const { id } of trackingImplementations) {
    if (!present.has(id)) continue;
    const files = (await readdir(join(root, id))).filter((file) => file.endsWith(".jsonl"));
    const list = (
      await Promise.all(
        files.map(async (file) => parseRuns(await readFile(join(root, id, file), "utf8"))),
      )
    ).flat();
    if (list.length) runs.set(id, list);
  }
  const growth = present.has(GROWTH_FILE)
    ? growthSchema.parse(JSON.parse(await readFile(join(root, GROWTH_FILE), "utf8")))
    : undefined;
  const data = comparisonData(runs, growth);
  await writeFile(join(root, "comparison.json"), `${JSON.stringify(data)}\n`);
  return data;
}
