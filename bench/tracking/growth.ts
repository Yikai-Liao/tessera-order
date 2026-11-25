// SPDX-License-Identifier: BSD-2-Clause

import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { z } from "zod";
import { command } from "../../tools/process.ts";
import { repositoryRoot, runTask } from "../../tools/tasks.ts";
import {
  caseId,
  implementation,
  operationSchema,
  parseCaseId,
  parseColumnId,
  route,
  SORT,
} from "../catalog.ts";
import { loadPlan, type Plan, parsePlan } from "../plan.ts";
import { withComparisonWorkspace } from "../release.ts";
import { memoryRows, verifiedPeak } from "../report/model.ts";
import {
  benchmarkEnvironment,
  buildBenchmark,
  resolveCommit,
  workingTreeDirty,
} from "../workspace.ts";
import { reportFailure } from "./failure.ts";
import { runSchema } from "./result.ts";

const sweepSchema = z
  .object({
    type: z.string().min(1),
    distribution: z.string().min(1),
    lengths: z
      .array(z.number().int().positive())
      .min(2)
      .refine((values) => values.every((n, index) => index === 0 || n > values[index - 1])),
    peaks: z.record(z.string().min(1), z.array(z.number().int().nonnegative().nullable())),
  })
  .refine(
    ({ peaks, lengths }) =>
      Object.values(peaks).every((values) => values.length === lengths.length),
    "Every implementation needs one peak entry per length",
  );

const provenanceSchema = z.object({
  time: z.iso.datetime(),
  hash: z.string().regex(/^[a-f0-9]{40}$/),
  workflow: runSchema.shape.workflow,
  rustc: z.string(),
  native: z.object({ cc: z.string(), cxx: z.string(), build: z.json() }),
});

export const growthSchema = provenanceSchema.extend({
  sweeps: z
    .array(sweepSchema.safeExtend({ operation: operationSchema }))
    .min(1)
    .refine(
      (sweeps) => new Set(sweeps.map(({ operation }) => operation.kind)).size === sweeps.length,
      "Each operation may have only one growth sweep",
    ),
});
export type GrowthRecord = z.infer<typeof growthSchema>;

/** Give each growth include rule its own serial memory session. */
export function growthPlans(plan: Plan): Plan[] {
  return plan.include.map((rule) => {
    const operation = rule.operation ?? SORT;
    const implementations = plan.implementations.filter(
      (id) =>
        id === "tessera" ||
        (implementation(id).parallel !== "only" &&
          (operation.kind === "sort" || route(id, operation))),
    );
    return parsePlan({ ...plan, implementations, include: [rule] });
  });
}

/**
 * Arrange one memory session's serial rows as one peak per implementation and length.
 *
 * The growth plan selects one operation, type and input pattern; an implementation that does not support
 * the type has no rows and no entry, and a missing length is null.
 */
export function growthPeaks(journal: string, plan: Plan) {
  const [rule] = plan.include;
  const [type] = rule.types ?? [];
  const [distribution] = rule.distributions ?? [];
  const lengths = (rule.lengths ?? []).filter((n): n is number => typeof n === "number");
  const operation = rule.operation ?? SORT;
  if (plan.include.length !== 1 || rule.types?.length !== 1 || rule.distributions?.length !== 1)
    throw new Error("The growth plan selects exactly one operation, type and input pattern");
  const positions = new Map(
    lengths.map((len, index) => [caseId({ ty: type, distribution, len, operation }), index]),
  );
  const peaks: Record<string, (number | null)[]> = {};
  for (const row of memoryRows(journal)) {
    const at = positions.get(caseId(parseCaseId(row.case_id)));
    const { implementation, execution } = parseColumnId(row.column);
    if (row.build !== "default" || execution !== "serial" || at === undefined) continue;
    if (!plan.implementations.includes(implementation))
      throw new Error(`Unplanned implementation: ${implementation}`);
    const peak = verifiedPeak(row);
    peaks[implementation] ??= lengths.map(() => null);
    const values = peaks[implementation];
    if (values[at] !== null) throw new Error(`Duplicate memory row: ${row.case_id} ${row.column}`);
    values[at] = peak;
  }
  if (!peaks.tessera?.every((value) => value !== null))
    throw new Error("Tessera needs a peak at every length");
  return { operation, type, distribution, lengths, peaks };
}

// CI-only, without credentials: measure every implementation's extra memory across the growth plan's
// lengths in one native build. Counts do not depend on the host, so this needs no Bencher run.
if (import.meta.main) {
  const root = await repositoryRoot();
  const out = join(root, "bench/results/growth");
  await reportFailure("Memory growth", `${out}.log`, () =>
    runTask(out, false, async () => {
      if (await workingTreeDirty(root))
        throw new Error("Growth requires a clean committed checkout");
      const hash = await resolveCommit(root, "HEAD");
      if (process.env.GITHUB_SHA !== hash || process.env.GITHUB_REF !== "refs/heads/main")
        throw new Error("Expected the exact main Actions source");
      await mkdir(out);
      return withComparisonWorkspace(root, true, async ({ tree, target }) => {
        const env = await benchmarkEnvironment(tree, {
          targetTriple: "x86_64-unknown-linux-gnu",
          env: { CC: "clang", CXX: "clang++" },
        });
        const memory = await buildBenchmark(
          tree,
          target,
          "memory",
          ["operations", "rayon", "compare-native"],
          { profile: "bench", targetTriple: "x86_64-unknown-linux-gnu", env },
        );
        const sweeps = [];
        for (const plan of growthPlans(await loadPlan("growth"))) {
          const kind = (plan.include[0].operation ?? SORT).kind;
          const path = join(out, `${kind}.json`);
          const directory = join(out, kind);
          await writeFile(path, JSON.stringify(plan));
          await command(memory.executable, ["--plan", path, "--out", directory]);
          sweeps.push(growthPeaks(await readFile(join(directory, "memory.jsonl"), "utf8"), plan));
        }
        const { provenance } = memory;
        const record: GrowthRecord = growthSchema.parse({
          time: new Date().toISOString(),
          hash,
          workflow: `${process.env.GITHUB_SERVER_URL}/${process.env.GITHUB_REPOSITORY}/actions/runs/${process.env.GITHUB_RUN_ID}/attempts/${process.env.GITHUB_RUN_ATTEMPT}`,
          rustc: provenance.rustc,
          native: { cc: provenance.cc, cxx: provenance.cxx, build: provenance.native_build },
          sweeps,
        });
        await writeFile(join(out, "growth.json"), JSON.stringify(record));
        return {
          hash,
          sweeps: sweeps.map(({ operation, lengths, peaks }) => ({
            operation: operation.kind,
            lengths: lengths.length,
            implementations: Object.keys(peaks).length,
          })),
        };
      });
    }),
  );
}
