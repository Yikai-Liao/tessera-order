// SPDX-License-Identifier: BSD-2-Clause

import { z } from "zod";
import type { RowChart } from "../plot/rows.ts";

const positive = z.number().finite().positive();
const metric = z
  .object({ value: positive, lower_value: positive, upper_value: positive })
  .refine((m) => m.lower_value <= m.value && m.value <= m.upper_value);
const reportSchema = z.object({
  uuid: z.uuid(),
  start_time: z.iso.datetime(),
  adapter: z.literal("rust_criterion"),
  project: z.object({ slug: z.string() }),
  branch: z.object({
    name: z.string(),
    head: z.object({ version: z.object({ hash: z.string() }) }),
  }),
  testbed: z.object({
    name: z.literal("main"),
    spec: z.object({ uuid: z.uuid(), slug: z.literal("intel-v1") }),
  }),
  results: z.tuple([
    z
      .array(
        z.object({
          benchmark: z.object({ name: z.string() }),
          measures: z.tuple([
            z.object({
              measure: z.object({
                slug: z.literal("latency"),
                units: z.literal("nanoseconds (ns)"),
              }),
              metric,
            }),
          ]),
        }),
      )
      .min(1),
  ]),
});

export const runSchema = z
  .object({
    id: z.uuid(),
    time: z.iso.datetime(),
    // Git committer time, normalized to UTC.
    commit_time: z.iso.datetime(),
    hash: z.string().regex(/^[a-f0-9]{40}$/),
    spec: z.uuid(),
    image: z.string().regex(/^registry\.bencher\.dev\/[a-z0-9._/-]+@sha256:[a-f0-9]{64}$/),
    workflow: z
      .url()
      .regex(/^https:\/\/github\.com\/[\w.-]+\/[\w.-]+\/actions\/runs\/\d+(?:\/attempts\/\d+)?$/),
    rustc: z.string(),
    rustflags: z.string(),
    // C++ adapters only: the compilers and flags that built the measured code.
    native: z.object({ cc: z.string(), cxx: z.string(), build: z.json() }).optional(),
    values: z.record(
      z.string().min(1),
      z.tuple([positive, positive, positive]).refine(([v, lo, hi]) => lo <= v && v <= hi),
    ),
    // Peak extra heap bytes of one sort per case, beyond the input; runs before memory
    // tracking have none. When present it covers exactly the timed cases.
    memory: z.record(z.string().min(1), z.number().int().nonnegative()).optional(),
  })
  .refine(
    (run) =>
      !run.memory ||
      (Object.keys(run.memory).length === Object.keys(run.values).length &&
        Object.keys(run.memory).every((id) => id in run.values)),
    "Memory must cover exactly the timed cases",
  );
export type TrackingRun = z.infer<typeof runSchema>;
/** A case's catalog presentation; the browser cannot read the catalog, so publication supplies it. */
export const caseSchema = z.object({
  id: z.string().min(1),
  label: z.string().min(1),
  group: z.string().min(1),
  input: z.string().min(1),
  sub: z.string(),
  tag: z.string(),
  type: z.string().min(1),
  length: z.string().min(1),
});
export type TrackingCase = z.infer<typeof caseSchema>;
/** Months with journals, cases in display order (grouped by mode, serial first)
 * and, for each case field a reader filters by, its values in catalog order.
 * `input` is the distribution and `group` the API mode. */
export const indexSchema = z.object({
  months: z.array(z.string().regex(/^\d{4}-(0[1-9]|1[0-2])$/)),
  cases: z.array(caseSchema),
  facets: z.object({
    type: z.array(z.string()),
    input: z.array(z.string()),
    length: z.array(z.string()),
    group: z.array(z.string()),
  }),
});
export type Facet = keyof z.infer<typeof indexSchema>["facets"];

/** Accept a complete absolute-time report for this source and inventory, or fail publication. */
export function summarizeReport(
  input: unknown,
  hash: string,
  project: string,
  ids: string[],
  branch: string,
) {
  const report = reportSchema.parse(input);
  if (
    report.branch.name !== branch ||
    report.branch.head.version.hash !== hash ||
    report.project.slug !== project
  )
    throw new Error("Tracking report source/project mismatch");
  const expected = new Set(ids);
  if (!ids.length || expected.size !== ids.length)
    throw new Error("Invalid expected tracking inventory");
  const values: TrackingRun["values"] = {};
  for (const entry of report.results[0]) {
    const id = entry.benchmark.name;
    if (!expected.delete(id)) throw new Error("Unexpected or duplicate case: " + id);
    const m = entry.measures[0].metric;
    values[id] = [m.value, m.lower_value, m.upper_value];
  }
  if (expected.size) throw new Error("Incomplete tracking report");
  return { id: report.uuid, time: report.start_time, hash, spec: report.testbed.spec.uuid, values };
}

/** Merge one run's remote jobs; the first job supplies the run's identity and start time. */
export function combineReports(jobs: ReturnType<typeof summarizeReport>[]) {
  const [first, ...rest] = jobs;
  if (!first || rest.some((job) => job.hash !== first.hash || job.spec !== first.spec))
    throw new Error("Tracking jobs disagree on source or hardware");
  // Each job was checked against its own disjoint inventory, so the values cannot collide.
  return { ...first, values: Object.assign({}, ...jobs.map((job) => job.values)) };
}

/** JSONL is the only stored measurement representation, shared by writers and the browser. */
export function parseRuns(text: string): TrackingRun[] {
  if (text && !text.endsWith("\n")) throw new Error("Truncated performance journal");
  return text.trim()
    ? text
        .trimEnd()
        .split("\n")
        .map((line) => runSchema.parse(JSON.parse(line)))
    : [];
}

const figureSchema = z.object({
  name: z.string().min(1),
  title: z.string().min(1),
  // Built and validated by the shared row template at publication; the page only renders it.
  chart: z.custom<RowChart>((value) => typeof value === "object" && value !== null),
});
/** Cross-algorithm figures from each algorithm's latest run, published as `comparison.json`. */
export const comparisonSchema = z.object({
  spec: z.uuid(),
  runs: z.array(
    z.object({
      algorithm: z.string().min(1),
      label: z.string().min(1),
      hash: z.string().regex(/^[a-f0-9]{40}$/),
      time: z.iso.datetime(),
      workflow: z.url(),
      memory: z.boolean(),
    }),
  ),
  overview: figureSchema.nullable(),
  figures: z.array(figureSchema),
});
export type ComparisonData = z.infer<typeof comparisonSchema>;
