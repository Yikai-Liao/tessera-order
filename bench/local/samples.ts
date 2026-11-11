// SPDX-License-Identifier: BSD-2-Clause

import { platform } from "node:os";
import { basename, dirname } from "node:path";
import { command, diagnostic } from "../../tools/process.ts";
import { parseCriterionId, revisionSampling } from "../catalog.ts";
import type { LocalRun } from "./model.ts";

/** Measure adjacent AB/BA samples in fresh worker pairs, retaining one total case budget. */
export async function measureSamples(
  run: LocalRun,
  binaries: { baseline: string; current: string },
  planPath: string,
  cpu: string | undefined,
  save: () => Promise<void>,
) {
  run.sampleCounts = Object.fromEntries(
    run.cases.map((id) => [id, revisionSampling(parseCriterionId(id).len).samples]),
  );
  run.samples = {};
  await save();
  const mixed = (run.shard?.allCases ?? run.cases).some(
    (id) => parseCriterionId(id).mode === "par-4",
  );
  for (const id of run.cases) {
    diagnostic(`Sampling ${id}`);
    const parallel = parseCriterionId(id).mode === "par-4";
    const mask = parallel || !mixed ? cpu : cpu?.split(",")[0];
    const count = run.sampleCounts[id];
    const worker = (
      role: keyof typeof binaries,
      interact: (exchange: (request: string) => Promise<string>) => Promise<void>,
    ) => {
      // Identical argv shape avoids changing the initial stack layout by revision name.
      const binary = binaries[role];
      const argv = [...(mask ? ["taskset", "-c", mask] : []), `./${basename(binary)}`, "--bench"];
      if (platform() === "linux") argv.unshift("setarch", "-R");
      return command(argv[0], argv.slice(1), {
        cwd: dirname(binary),
        interact,
        env: {
          TESSERA_BENCH_PAIRED: "1",
          TESSERA_BENCH_IDS: id,
          TESSERA_BENCH_PLAN: planPath,
          TESSERA_BENCH_PARALLEL: parallel ? "4" : undefined,
        },
      });
    };
    const samples: { baseline: number; current: number }[] = [];
    for (let index = 0; index < count; index++) {
      const roles =
        index % 2 ? (["current", "baseline"] as const) : (["baseline", "current"] as const);
      const pair = { baseline: 0, current: 0 };
      await worker(roles[0], async (a) => {
        if (JSON.parse(await a("start")).id !== id) throw new Error("Wrong first case");
        await worker(roles[1], async (b) => {
          if (JSON.parse(await b("start")).id !== id) throw new Error("Wrong second case");
          for (const exchange of [a, b]) {
            const warm = JSON.parse(await exchange("warm"));
            if (warm.id !== id || warm.samples !== count)
              throw new Error("Incompatible sample budgets");
          }
          for (const [role, exchange] of [
            [roles[0], a],
            [roles[1], b],
          ] as const) {
            const value = JSON.parse(await exchange("sample"));
            if (![value.elapsed_ns, value.sorts].every((v) => Number.isSafeInteger(v) && v > 0))
              throw new Error("Invalid worker observation");
            pair[role] = value.elapsed_ns / value.sorts;
          }
          await b("stop");
        });
        await a("stop");
      });
      samples.push(pair);
    }
    run.samples[id] = samples;
    await save();
  }
}
