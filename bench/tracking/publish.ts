// SPDX-License-Identifier: BSD-2-Clause

import { mkdir, readdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { caseLabel, caseName, caseParts, compareCases, modeName } from "../catalog.ts";
import { renderMainOverview } from "./history.ts";
import {
  indexSchema,
  parseRuns,
  runSchema,
  type TrackingCase,
  type TrackingRun,
} from "./result.ts";

/** Present cases from the catalog in reading order; historical IDs keep readable fallbacks. */
export function trackingCases(ids: string[]): TrackingCase[] {
  return ids.toSorted(compareCases).map((id) => ({
    id,
    label: caseLabel(id),
    group: modeName(caseParts(id).mode) || "other",
    ...caseName(id),
  }));
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
  // Validate what the browser will parse, so a bad index fails publication instead of the page.
  const index = indexSchema.parse({
    months: [...months.keys()],
    cases: trackingCases([...new Set(runs.flatMap((entry) => Object.keys(entry.values)))]),
  });
  await writeFile(join(directory, "index.json"), JSON.stringify(index) + "\n");
  await writeFile(join(directory, "preview.svg"), renderMainOverview(runs, index.cases));
}

if (import.meta.main) {
  const [run] = parseRuns(await readFile(process.argv[2], "utf8"));
  await appendRun("main", run);
}
