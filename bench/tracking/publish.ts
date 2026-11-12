// SPDX-License-Identifier: BSD-2-Clause

import { mkdir, readdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { caseLabel } from "../catalog.ts";
import { renderHistory } from "./history.ts";
import { PREVIEW_CASE, parseRuns, runSchema, type TrackingRun } from "./result.ts";

/** Append a validated report and regenerate the index/preview. Git owns atomic publication and retries. */
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
    return;
  }
  runs.push(run);
  runs.sort((a, b) => a.time.localeCompare(b.time));
  const months = Map.groupBy(runs, (entry) => entry.time.slice(0, 7));
  for (const [month, entries] of months)
    await writeFile(
      join(directory, month + ".jsonl"),
      entries.map((entry) => JSON.stringify(entry) + "\n").join(""),
    );
  const index = {
    months: [...months.keys()],
    cases: [...new Set(runs.flatMap((entry) => Object.keys(entry.values)))]
      .sort()
      .map((id) => ({ id, label: caseLabel(id) })),
  };
  await writeFile(join(directory, "index.json"), JSON.stringify(index) + "\n");
  await writeFile(join(directory, "preview.svg"), renderHistory(runs, PREVIEW_CASE));
}

if (import.meta.main) {
  const [run] = parseRuns(await readFile(process.argv[2], "utf8"));
  await appendRun("main", run);
}
