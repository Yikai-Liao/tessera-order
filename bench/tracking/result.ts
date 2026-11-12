// SPDX-License-Identifier: BSD-2-Clause

import { z } from "zod";

export const PREVIEW_CASE = "rust/u64/random/65536/s1/api";

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
    name: z.literal("main"),
    head: z.object({ version: z.object({ hash: z.string() }) }),
  }),
  testbed: z.object({
    name: z.literal("baremetal-intel-v1-absolute-v1"),
    spec: z.object({ uuid: z.uuid(), slug: z.literal("intel-v1") }),
  }),
  results: z
    .array(
      z
        .array(
          z.object({
            benchmark: z.object({ name: z.string() }),
            measures: z
              .array(
                z.object({
                  measure: z.object({
                    slug: z.literal("latency"),
                    units: z.literal("nanoseconds (ns)"),
                  }),
                  metric,
                }),
              )
              .length(1),
          }),
        )
        .min(1),
    )
    .length(1),
});

export const runSchema = z.object({
  id: z.uuid(),
  time: z.iso.datetime(),
  hash: z.string().regex(/^[a-f0-9]{40}$/),
  spec: z.uuid(),
  protocol: z.literal("criterion-main-v1"),
  image: z.string().regex(/^registry\.bencher\.dev\/[a-z0-9._/-]+@sha256:[a-f0-9]{64}$/),
  workflow: z
    .url()
    .regex(/^https:\/\/github\.com\/[\w.-]+\/[\w.-]+\/actions\/runs\/\d+(?:\/attempts\/\d+)?$/),
  rustc: z.string(),
  rustflags: z.string(),
  plan_hash: z.string(),
  executable_hash: z.string(),
  lock_hash: z.string(),
  values: z.record(
    z.string().min(1),
    z.tuple([positive, positive, positive]).refine(([v, lo, hi]) => lo <= v && v <= hi),
  ),
});
export type TrackingRun = z.infer<typeof runSchema>;
export const indexSchema = z.object({
  months: z.array(
    z.object({
      month: z.string().regex(/^\d{4}-(0[1-9]|1[0-2])$/),
      file: z.string().regex(/^\d{4}-\d{2}-[a-f0-9]{64}\.jsonl$/),
    }),
  ),
  cases: z.array(z.object({ id: z.string(), label: z.string() })),
});

/** Accept a complete absolute-time report for this source and inventory, or fail publication. */
export function summarizeReport(input: unknown, hash: string, project: string, ids: string[]) {
  const report = reportSchema.parse(input);
  if (report.branch.head.version.hash !== hash || report.project.slug !== project)
    throw new Error("Tracking report source/project mismatch");
  const expected = new Set(ids);
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
