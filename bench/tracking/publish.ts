// SPDX-License-Identifier: BSD-2-Clause

import { mkdir, readdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import {
  caseLabel,
  caseName,
  caseParts,
  catalog,
  compareCases,
  modeName,
  modeOrder,
} from "../catalog.ts";
import { trackingAlgorithm, trackingAlgorithms } from "./algorithms.ts";
import { renderMainOverview } from "./history.ts";
import {
  type Facet,
  indexSchema,
  parseRuns,
  runSchema,
  type TrackingCase,
  type TrackingRun,
} from "./result.ts";

/** Present cases from the catalog in reading order; historical IDs keep readable fallbacks. */
export function trackingCases(ids: string[]): TrackingCase[] {
  return ids.toSorted(compareCases).map((id) => {
    const parts = caseParts(id);
    return {
      id,
      label: caseLabel(id),
      group: modeName(parts.mode) || "other",
      ...caseName(id),
      type: parts.type || "other",
      length: parts.n || "other",
    };
  });
}

/** Each filterable field's values in catalog order; unknown values follow, in reading order. */
export function trackingFacets(cases: TrackingCase[]) {
  const part = (id: string, index: number) => id.split("/")[index] ?? "";
  const rank = (list: { id: string }[], value: string) => {
    const found = list.findIndex((entry) => entry.id === value);
    return found < 0 ? list.length : found;
  };
  const ordered = (field: Facet, key: (id: string) => number) => {
    const first = new Map<string, number>();
    for (const entry of cases) if (!first.has(entry[field])) first.set(entry[field], key(entry.id));
    // A stable sort keeps reading order among equal keys.
    return [...first].toSorted((a, b) => a[1] - b[1]).map(([value]) => value);
  };
  return {
    type: ordered("type", (id) => rank(catalog.types, part(id, 1))),
    input: ordered("input", (id) => rank(catalog.distributions, part(id, 2))),
    length: ordered("length", (id) => Number(part(id, 3)) || Number.MAX_SAFE_INTEGER),
    group: ordered("group", (id) => modeOrder(part(id, 5))),
  };
}

/** Append a validated report and regenerate the index/preview. Git owns atomic publication and retries.
 *
 * Re-publishing an identical report regenerates the derived files without remeasuring.
 */
export async function appendRun(directory: string, input: TrackingRun | TrackingRun[]) {
  const incoming = (Array.isArray(input) ? input : [input]).map((run) => runSchema.parse(run));
  await mkdir(directory, { recursive: true });
  const files = (await readdir(directory)).filter((file) => file.endsWith(".jsonl"));
  const runs = (
    await Promise.all(
      files.map(async (file) => parseRuns(await readFile(join(directory, file), "utf8"))),
    )
  ).flat();
  for (const run of incoming) {
    const previous = runs.find((entry) => entry.id === run.id);
    if (previous) {
      if (JSON.stringify(previous) !== JSON.stringify(run))
        throw new Error("Report identity has different data");
    } else runs.push(run);
  }
  if (new Set(runs.map((run) => run.algorithm)).size > 1)
    throw new Error("Cannot mix algorithms in one journal");
  runs.sort((a, b) => a.time.localeCompare(b.time));
  const months = Map.groupBy(runs, (entry) => entry.time.slice(0, 7));
  for (const [month, entries] of months)
    await writeFile(
      join(directory, month + ".jsonl"),
      entries.map((entry) => JSON.stringify(entry) + "\n").join(""),
    );
  const cases = trackingCases([...new Set(runs.flatMap((entry) => Object.keys(entry.values)))]);
  // Validate what the browser will parse, so a bad index fails publication instead of the page.
  const index = indexSchema.parse({
    months: [...months.keys()],
    cases,
    facets: trackingFacets(cases),
  });
  await writeFile(join(directory, "index.json"), JSON.stringify(index) + "\n");
  if (runs.every((run) => run.algorithm === "tessera"))
    await writeFile(join(directory, "preview.svg"), renderMainOverview(runs, index.cases));
}

/** Publish one algorithm namespace and regenerate the catalog-derived root index.
 * The caller owns the Git transaction and retries competing ref updates. */
export async function publishRun(root: string, input: TrackingRun) {
  const run = runSchema.parse(input);
  const algorithm = trackingAlgorithm(run.algorithm);
  await appendRun(join(root, algorithm.path), run);
  const available = [];
  for (const entry of trackingAlgorithms) {
    const index = await readFile(join(root, entry.path, "index.json"), "utf8").catch(
      (error: NodeJS.ErrnoException) => {
        if (error.code !== "ENOENT") throw error;
        return null;
      },
    );
    if (index === null) continue;
    indexSchema.parse(JSON.parse(index));
    available.push({
      id: entry.path,
      label: entry.label,
      path: entry.path + "/",
      stable: entry.stable,
      family: entry.family,
      ...(entry.threads ? { threads: entry.threads } : {}),
    });
  }
  await writeFile(join(root, "index.json"), JSON.stringify({ algorithms: available }) + "\n");
}

if (import.meta.main) {
  const runs = parseRuns(await readFile(process.argv[2], "utf8"));
  if (runs.length !== 1) throw new Error("Expected exactly one complete algorithm summary");
  await publishRun(".", runs[0]);
}
