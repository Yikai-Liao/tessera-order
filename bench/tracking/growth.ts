// SPDX-License-Identifier: BSD-2-Clause

import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { command } from "../../tools/process.ts";
import { repositoryRoot, runTask } from "../../tools/tasks.ts";
import { loadPlan, type Plan } from "../plan.ts";
import { withComparisonWorkspace } from "../release.ts";
import {
  benchmarkEnvironment,
  buildBenchmark,
  resolveCommit,
  workingTreeDirty,
} from "../workspace.ts";
import { reportFailure } from "./failure.ts";
import { type GrowthRecord, growthSchema } from "./result.ts";

/**
 * Arrange the memory worker's rows as one peak per algorithm and length.
 *
 * The growth plan selects one type and one input pattern; an algorithm that does not support
 * the type has no rows and no entry, and a missing length is null.
 */
export function growthPeaks(journal: string, plan: Plan) {
  const [rule] = plan.include;
  const [type] = rule.types ?? [];
  const [distribution] = rule.distributions ?? [];
  const lengths = (rule.lengths ?? []).filter((n): n is number => typeof n === "number");
  if (plan.include.length !== 1 || rule.types?.length !== 1 || rule.distributions?.length !== 1)
    throw new Error("The growth plan selects exactly one type and one input pattern");
  const peaks: Record<string, (number | null)[]> = {};
  for (const line of journal.split("\n").filter(Boolean)) {
    const row = JSON.parse(line) as {
      case_id: string;
      variant: string;
      build: string;
      peak_extra_bytes: number;
      verified: boolean;
    };
    const [ty, shape, len] = row.case_id.split("/");
    const at = lengths.indexOf(Number(len));
    if (row.build !== "default" || ty !== type || shape !== distribution || at < 0) continue;
    if (!plan.algorithms.includes(row.variant))
      throw new Error(`Unplanned variant: ${row.variant}`);
    if (!row.verified || !Number.isSafeInteger(row.peak_extra_bytes) || row.peak_extra_bytes < 0)
      throw new Error(`Invalid memory row: ${row.case_id} ${row.variant}`);
    peaks[row.variant] ??= lengths.map(() => null);
    const values = peaks[row.variant];
    if (values[at] !== null) throw new Error(`Duplicate memory row: ${row.case_id} ${row.variant}`);
    values[at] = row.peak_extra_bytes;
  }
  if (!peaks.tessera?.every((value) => value !== null))
    throw new Error("Tessera needs a peak at every length");
  return { type, distribution, lengths, peaks };
}

// CI-only, without credentials: measure every algorithm's extra memory across the growth plan's
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
        const memory = await buildBenchmark(tree, target, "memory", ["rayon", "compare-native"], {
          profile: "bench",
          targetTriple: "x86_64-unknown-linux-gnu",
          env,
        });
        const plan = await loadPlan("growth");
        await writeFile(join(out, "plan.json"), JSON.stringify(plan));
        await command(memory.executable, [
          "--plan",
          join(out, "plan.json"),
          "--out",
          join(out, "scratch"),
        ]);
        const { provenance } = memory;
        const record: GrowthRecord = growthSchema.parse({
          time: new Date().toISOString(),
          hash,
          workflow: `${process.env.GITHUB_SERVER_URL}/${process.env.GITHUB_REPOSITORY}/actions/runs/${process.env.GITHUB_RUN_ID}/attempts/${process.env.GITHUB_RUN_ATTEMPT}`,
          rustc: provenance.rustc,
          native: { cc: provenance.cc, cxx: provenance.cxx, build: provenance.native_build },
          ...growthPeaks(await readFile(join(out, "scratch/memory.jsonl"), "utf8"), plan),
        });
        await writeFile(join(out, "growth.json"), JSON.stringify(record));
        return {
          hash,
          lengths: record.lengths.length,
          algorithms: Object.keys(record.peaks).length,
        };
      });
    }),
  );
}
