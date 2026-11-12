// SPDX-License-Identifier: BSD-2-Clause

import { mkdir, readdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { caseParts, catalog, modeName, modeOrder } from "../catalog.ts";
import { renderMainOverview } from "./history.ts";
import { parseRuns, runSchema, type TrackingCase, type TrackingRun } from "./result.ts";

const position = (list: { id: string }[], id: string) => list.findIndex((entry) => entry.id === id);

/** Present cases from the catalog in reading order: mode, length, record type, then input.
 *
 * Journals outlive catalog entries, so unknown fields keep their raw names instead of failing.
 */
export function trackingCases(ids: string[]): TrackingCase[] {
  return ids
    .map((id) => {
      const [, ty = "", distribution = "", len = "0", seed = "s1", mode = ""] = id.split("/");
      return { id, key: { ty, distribution, len: Number(len), seed, mode }, parts: caseParts(id) };
    })
    .sort(
      (a, b) =>
        modeOrder(a.key.mode) - modeOrder(b.key.mode) ||
        a.key.len - b.key.len ||
        position(catalog.types, a.key.ty) - position(catalog.types, b.key.ty) ||
        position(catalog.distributions, a.key.distribution) -
          position(catalog.distributions, b.key.distribution) ||
        a.id.localeCompare(b.id),
    )
    .map(({ id, key, parts }) => ({
      id,
      group: modeName(key.mode),
      input: parts.input,
      sub: [parts.type, parts.n].filter(Boolean).join(" · "),
      tag: key.seed === "s1" ? "" : `seed ${key.seed.slice(1)}`,
    }));
}

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
    cases: trackingCases([...new Set(runs.flatMap((entry) => Object.keys(entry.values)))]),
  };
  await writeFile(join(directory, "index.json"), JSON.stringify(index) + "\n");
  await writeFile(join(directory, "preview.svg"), renderMainOverview(runs, index.cases));
}

if (import.meta.main) {
  const [run] = parseRuns(await readFile(process.argv[2], "utf8"));
  await appendRun("main", run);
}
