// SPDX-License-Identifier: BSD-2-Clause

import { platform } from "node:os";
import { command, diagnostic } from "../../tools/process.ts";
import { parseCriterionId, revisionSampling } from "../catalog.ts";
import type { LocalRun } from "./model.ts";

/** Own both worker lifetimes; only one prepares or measures at a time.
 *
 * Each worker retains one case and its pool. Requests exclude restoration,
 * verification and IPC from the sort clock; AB/BA alternates sample order.
 */
export async function measureSamples(
  run: LocalRun,
  binaries: { baseline: string; current: string },
  cwd: string,
  planPath: string,
  cpu: string | undefined,
  save: () => Promise<void>,
) {
  run.samples = {};
  for (const id of run.cases) {
    diagnostic(`Sampling ${id}`);
    const parallel = parseCriterionId(id).mode === "par-4";
    const mask = parallel ? cpu : cpu?.split(",")[0];
    const worker = (
      binary: string,
      interact: (exchange: (request: string) => Promise<string>) => Promise<void>,
    ) => {
      const argv = [...(mask ? ["taskset", "-c", mask] : []), binary, "--bench"];
      if (platform() === "linux") argv.unshift("setarch", "-R");
      return command(argv[0], argv.slice(1), {
        cwd,
        interact,
        env: {
          TESSERA_BENCH_PAIRED: "1",
          TESSERA_BENCH_IDS: id,
          TESSERA_BENCH_PLAN: planPath,
          TESSERA_BENCH_PARALLEL: parallel ? "4" : undefined,
        },
      });
    };
    await worker(binaries.baseline, async (a) => {
      if (JSON.parse(await a("start")).id !== id) throw new Error("Wrong baseline case");
      await worker(binaries.current, async (b) => {
        if (JSON.parse(await b("start")).id !== id) throw new Error("Wrong current case");
        const first = JSON.parse(await a("warm"));
        const second = JSON.parse(await b("warm"));
        if (
          first.id !== id ||
          second.id !== id ||
          first.samples !== second.samples ||
          first.samples !== revisionSampling(parseCriterionId(id).len).samples
        )
          throw new Error("Incompatible sample budgets");
        const samples: { baseline: number; current: number }[] = [];
        const sample = async (exchange: typeof a) => {
          const value = JSON.parse(await exchange("sample"));
          if (![value.elapsed_ns, value.sorts].every((v) => Number.isSafeInteger(v) && v > 0))
            throw new Error("Invalid worker observation");
          return value.elapsed_ns / value.sorts;
        };
        for (let index = 0; index < first.samples; index++) {
          let baseline: number, current: number;
          if (index % 2) {
            current = await sample(b);
            baseline = await sample(a);
          } else {
            baseline = await sample(a);
            current = await sample(b);
          }
          samples.push({ baseline, current });
        }
        await b("stop");
        run.samples![id] = samples;
      });
      await a("stop");
    });
    await save();
  }
}
