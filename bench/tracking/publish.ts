// SPDX-License-Identifier: BSD-2-Clause

import { createHash } from "node:crypto";
import { mkdir, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { caseLabel } from "../catalog.ts";
import { renderHistory } from "../plot/history.ts";
import { indexSchema, PREVIEW_CASE, parseRuns, runSchema, type TrackingRun } from "./result.ts";

/** Update a checked-out data branch. Git owns races/retries; no API publisher or source manifest.
 * Content-addressed monthly files prevent a cached index from silently selecting different data.
 */
export async function appendRun(directory: string, input: TrackingRun) {
  const run = runSchema.parse(input);
  await mkdir(directory, { recursive: true });
  const files = (await readdir(directory)).filter((file) => file.endsWith(".jsonl")).sort();
  const records = await Promise.all(
    files.map(async (file) => ({
      file,
      runs: parseRuns(await readFile(join(directory, file), "utf8")),
    })),
  );
  const previous = records.flatMap((entry) => entry.runs).find((entry) => entry.id === run.id);
  if (previous) {
    if (JSON.stringify(previous) !== JSON.stringify(run))
      throw new Error("Report identity has different data");
    return;
  }
  const month = run.time.slice(0, 7);
  const old = records.find((entry) => entry.file.startsWith(month + "-"));
  const runs = [...(old?.runs ?? []), run].sort((a, b) => a.time.localeCompare(b.time));
  const text = runs.map((entry) => JSON.stringify(entry) + "\n").join("");
  const file = month + "-" + createHash("sha256").update(text).digest("hex") + ".jsonl";
  await writeFile(join(directory, file), text);
  if (old) await rm(join(directory, old.file));
  const next = [...records.filter((entry) => entry !== old), { file, runs }].sort((a, b) =>
    a.file.localeCompare(b.file),
  );
  const ids = [
    ...new Set(next.flatMap((entry) => entry.runs.flatMap((r) => Object.keys(r.values)))),
  ].sort();
  const index = indexSchema.parse({
    months: next.map((entry) => ({ month: entry.file.slice(0, 7), file: entry.file })),
    cases: ids.map((id) => ({ id, label: caseLabel(id) })),
  });
  await writeFile(join(directory, "index.json"), JSON.stringify(index) + "\n");
  const points = next
    .flatMap((entry) => entry.runs)
    .filter((r) => r.values[PREVIEW_CASE])
    .map((r) => ({
      time: new Date(r.time),
      value: r.values[PREVIEW_CASE][0],
      lower: r.values[PREVIEW_CASE][1],
      upper: r.values[PREVIEW_CASE][2],
      series: r.protocol + "/" + r.spec,
      title: r.time + " · " + r.hash.slice(0, 7),
      href: "https://github.com/Yikai-Liao/tessera-sort/commit/" + r.hash,
    }));
  await writeFile(join(directory, "preview.svg"), renderHistory(points));
}

if (import.meta.main) {
  const [run] = parseRuns(await readFile(process.argv[2], "utf8"));
  await appendRun("main", run);
}
