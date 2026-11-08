// SPDX-License-Identifier: BSD-2-Clause

import { type ChildProcess, spawn } from "node:child_process";
import { createReadStream } from "node:fs";
import { glob, mkdir, mkdtemp, open, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pipeline } from "node:stream/promises";
import { pathToFileURL } from "node:url";
import { CommandCancelledError, command } from "../../tools/process.ts";

/** Balance API cases across two small lanes; run large cases separately. */
export function partition(cases: string[]) {
  const small: string[][] = [[], []];
  const large: string[] = [];
  for (const id of cases.toSorted()) {
    const group = id.slice(0, id.lastIndexOf("/"));
    if (Number(group.split("/")[3]) >= 1_000_000) large.push(group);
    else {
      const lane = small[0].length <= small[1].length ? 0 : 1;
      small[lane].push(group);
    }
  }
  return { small, large };
}

/** Start isolated workers and reap all children before returning or propagating failure. */
export async function runWorkers(
  binary: string,
  lanes: string[][],
  cpus: number[],
  directory: string,
) {
  const children: { child: ChildProcess; done: Promise<Error | null>; output: string }[] = [];
  let cancelled: CommandCancelledError | undefined;
  const stop = (signal: NodeJS.Signals) => {
    cancelled ??= new CommandCancelledError(signal);
    for (const { child } of children) child.kill("SIGKILL");
  };
  process.on("SIGINT", stop);
  process.on("SIGTERM", stop);
  try {
    for (const [lane, groups] of lanes.entries()) {
      if (!groups.length) continue;
      const output = join(directory, String(lane));
      await mkdir(output, { recursive: true });
      const log = await open(join(output, "stdout"), "wx");
      try {
        if (cancelled) throw cancelled;
        const child = spawn("taskset", ["-c", String(cpus[lane]), binary, "--bench"], {
          env: {
            ...process.env,
            TESSERA_BENCH_GROUPS: groups.join("\n"),
            CRITERION_HOME: join(output, "criterion"),
          },
          stdio: ["ignore", log.fd, "inherit"],
        });
        // Observe errors immediately: a worker can exit while the next log opens.
        const done = new Promise<Error | null>((resolve) => {
          child.once("error", resolve);
          child.once("close", (code, signal) =>
            resolve(code === 0 ? null : new Error(`Benchmark worker ended with ${signal ?? code}`)),
          );
        });
        children.push({ child, done, output });
      } finally {
        await log.close();
      }
    }
    await Promise.all(
      children.map(async ({ done }) => {
        const error = await done;
        if (error) throw error;
      }),
    );
    if (cancelled) throw cancelled;
    return children.map(({ output }) => output);
  } finally {
    // SIGKILL also reaps a sibling that ignores the supervisor's termination.
    for (const { child } of children)
      if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
    await Promise.all(children.map(({ done }) => done));
    process.removeListener("SIGINT", stop);
    process.removeListener("SIGTERM", stop);
  }
}

/** Read completed Criterion IDs, rejecting duplicate results across workers. */
export async function measuredCases(outputs: string[]) {
  const cases = new Set<string>();
  for (const output of outputs) {
    for await (const path of glob("**/new/benchmark.json", { cwd: join(output, "criterion") })) {
      const id = JSON.parse(await readFile(join(output, "criterion", path), "utf8")).full_id;
      if (typeof id !== "string" || cases.has(id))
        throw new Error("Invalid or duplicate benchmark results");
      cases.add(id);
    }
  }
  return cases;
}

/** Reproduce the cloud protocol: two pinned small workers, then one exclusive large worker. */
export async function supervise(binary: string) {
  if (process.platform !== "linux") throw new Error("Continuous scheduling requires Linux");
  if (
    process.env.TESSERA_BENCH_GROUPS !== undefined ||
    process.env.TESSERA_BENCH_LIST !== undefined
  )
    throw new Error("Worker selection variables must not be set on the supervisor");
  const cases = (
    await command(binary, ["--bench"], {
      capture: true,
      env: { TESSERA_BENCH_LIST: "1" },
    })
  )
    .split("\n")
    .filter(Boolean);
  if (!cases.length || new Set(cases).size !== cases.length)
    throw new Error("Expected distinct benchmark IDs");
  for (const id of cases)
    if (!/^rust\/[^/]+\/[^/]+\/[1-9]\d*\/[^/]+\/api$/.test(id))
      throw new Error(`Invalid benchmark ID: ${id}`);
  const allowed = (await readFile("/proc/self/status", "utf8")).match(
    /^Cpus_allowed_list:\s*(.+)$/m,
  )?.[1];
  const cpus: number[] = [];
  for (const range of (allowed ?? "").split(",")) {
    const [first, last = first] = range.split("-").map(Number);
    for (let cpu = first; cpu <= last && cpus.length < 2; cpu++) cpus.push(cpu);
    if (cpus.length === 2) break;
  }
  if (cpus.length !== 2) throw new Error("Two available CPUs are required");
  const { small, large } = partition(cases);
  const directory = await mkdtemp(join(tmpdir(), "tessera-bench-"));
  try {
    const outputs = await runWorkers(binary, small, cpus, join(directory, "small"));
    // Preparation and validation of large inputs must not overlap either small worker.
    outputs.push(...(await runWorkers(binary, [large], cpus, join(directory, "large"))));
    const measured = await measuredCases(outputs);
    if (measured.size !== cases.length || cases.some((id) => !measured.has(id)))
      throw new Error("Benchmark results differ from the declared suite");
    for (const output of outputs)
      await pipeline(createReadStream(join(output, "stdout")), process.stdout, { end: false });
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  if (!process.argv[2]) throw new Error("Usage: worker.ts EXECUTABLE");
  await supervise(resolve(process.argv[2]));
}
