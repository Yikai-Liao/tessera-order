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
import { trackingAlgorithm } from "./algorithms.ts";
import { writeComparison } from "./comparison.ts";
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
export async function appendRun(directory: string, input: TrackingRun) {
  const run = runSchema.parse(input);
  await mkdir(directory, { recursive: true });
  const files = (await readdir(directory)).filter((file) => file.endsWith(".jsonl"));
  const runs = (
    await Promise.all(
      files.map(async (file) => parseRuns(await readFile(join(directory, file), "utf8"))),
    )
  ).flat();
  const previous = runs.find((entry) => entry.id === run.id);
  if (previous) {
    if (JSON.stringify(previous) !== JSON.stringify(run))
      throw new Error("Report identity has different data");
  } else runs.push(run);
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
  await writeFile(join(directory, "preview.svg"), renderMainOverview(runs, index.cases));
}

// Usage: publish.ts ALGORITHM JOURNAL, run from the data checkout's root; also regenerates
// `comparison.json` from every algorithm present.
if (import.meta.main) {
  const [algorithm, journal] = process.argv.slice(2);
  const runs = parseRuns(await readFile(journal, "utf8"));
  if (runs.length !== 1) throw new Error("Expected exactly one complete run");
  await appendRun(trackingAlgorithm(algorithm).id, runs[0]);
  await writeComparison(".");
}
