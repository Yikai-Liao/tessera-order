// SPDX-License-Identifier: BSD-2-Clause

import { readFile } from "node:fs/promises";
import { platform } from "node:os";
import { basename, dirname } from "node:path";
import { command, diagnostic } from "../../tools/process.ts";
import { parseCriterionId, revisionSampling } from "../catalog.ts";
import { type LocalRun, shardCases } from "./model.ts";

/** Serial cases measured at once on a host reserved for four-thread cases. Each lane
 * takes one physical core, so lanes never share an SMT sibling's caches and ports. */
const SERIAL_LANES = 2;

/** One CPU per physical core in `mask`, in mask order, at most `SERIAL_LANES`. Without
 * readable topology the host keeps a single lane. */
export async function serialLanes(
  mask: string,
  core: (cpu: string) => Promise<string> = async (cpu) => {
    const topology = `/sys/devices/system/cpu/cpu${cpu}/topology`;
    const read = (name: string) => readFile(`${topology}/${name}`, "utf8");
    return `${(await read("physical_package_id")).trim()}:${(await read("core_id")).trim()}`;
  },
): Promise<string[]> {
  const lanes = new Map<string, string>();
  for (const cpu of mask.split(",")) {
    let id: string;
    try {
      id = await core(cpu);
    } catch {
      return [mask.split(",")[0]];
    }
    if (!lanes.has(id)) lanes.set(id, cpu);
  }
  return [...lanes.values()].slice(0, SERIAL_LANES);
}

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
    (id) => parseCriterionId(id).execution !== "serial",
  );
  // Concurrent lanes finish in any order; keep each snapshot write whole.
  let saving = Promise.resolve();
  const persist = () => {
    saving = saving.then(save);
    return saving;
  };
  const measureCase = async (id: string, mask: string | undefined) => {
    diagnostic(`Sampling ${id}${mask ? ` on CPU ${mask}` : ""}`);
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
    await persist();
  };
  // Parallel cases own every reserved CPU; serial lanes start only after them.
  const parallel = run.cases.filter((id) => parseCriterionId(id).execution !== "serial");
  const serial = run.cases.filter((id) => parseCriterionId(id).execution === "serial");
  for (const id of parallel) await measureCase(id, cpu);
  const lanes = mixed && cpu ? await serialLanes(cpu) : [cpu];
  const count = Math.min(lanes.length, serial.length);
  if (count > 1) run.host.serialLanes = lanes.slice(0, count) as string[];
  if (count <= 1) {
    for (const id of serial) await measureCase(id, mixed ? cpu?.split(",")[0] : cpu);
  } else {
    // A failed lane stops the others at their next case; each running worker is reaped.
    let failed = false;
    const results = await Promise.allSettled(
      lanes.slice(0, count).map(async (mask, index) => {
        for (const id of shardCases(serial, index + 1, count)) {
          if (failed) return;
          try {
            await measureCase(id, mask);
          } catch (error) {
            failed = true;
            throw error;
          }
        }
      }),
    );
    const failure = results.find((result) => result.status === "rejected");
    if (failure) throw failure.reason;
  }
  await saving;
}
