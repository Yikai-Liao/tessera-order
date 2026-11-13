// SPDX-License-Identifier: BSD-2-Clause

import { mkdir, symlink, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { command, diagnostic } from "../tools/process.ts";
import { catalog, inventorySchema } from "./catalog.ts";
import { loadPlan, type Plan, parsePlan } from "./plan.ts";
import { writeReport } from "./report/index.ts";
import { prepareRevisions, type Revisions } from "./revisions.ts";
import { fetchNative } from "./rust/native/fetch.ts";
import {
  executables,
  fileHash,
  loadSession,
  retainBuild,
  type Session,
  withSessionLock,
} from "./session.ts";
import { pairedBudget } from "./statistics.ts";
import {
  benchmarkEnvironment,
  buildBenchmark,
  copySnapshot,
  type Workspace,
  withBuildCache,
  withWorkspace,
} from "./workspace.ts";

/** Acquire native sources and own their link inside an isolated build workspace. */
export async function withComparisonWorkspace<T>(
  root: string,
  native: boolean,
  operation: (workspace: Workspace) => Promise<T>,
) {
  if (native) await fetchNative(root);
  return withWorkspace(root, async (workspace) => {
    if (native) {
      await symlink(join(root, "bench/vendor"), join(workspace.tree, "bench/vendor"), "dir");
      await symlink(join(root, "node_modules"), join(workspace.tree, "node_modules"), "dir");
      await copySnapshot(
        join(root, "tools/process.ts"),
        join(workspace.tree, "tools/process.ts"),
        root,
      );
    }
    return operation(workspace);
  });
}

/** Reproduce CI's harness tests, smoke validation, tiny timing run and offline report. */
export async function checkAlgorithms(root: string, out: string, native: boolean) {
  if (native && process.platform !== "linux")
    throw new Error("Native linkage checks require Linux and ldd");
  await mkdir(out, { recursive: false });
  await withComparisonWorkspace(root, native, async ({ tree, target }) => {
    const features = ["compare", "rayon", ...(native ? ["compare-native"] : [])];
    await prepareRevisions(root, tree, { baseline: "HEAD" }, features);
    await withBuildCache(target, async () => {
      const env = await benchmarkEnvironment(tree);
      for (const targets of [["--lib", "--bins", "--tests"], ["--doc"]])
        await command(
          "cargo",
          [
            "test",
            "--locked",
            "-p",
            "tessera-bench",
            "--features",
            features.join(","),
            ...targets,
            "--target-dir",
            target,
          ],
          { cwd: tree, env },
        );
    });
    const { executable } = await buildBenchmark(tree, target, "compare", features, {
      profile: "dev",
    });
    if (native) {
      for (const name of ["compare", "memory"]) {
        const binary = name === "compare" ? executable : join(dirname(executable), "memory");
        const libraries = await command("ldd", [binary], { capture: true });
        if (/libtbb|libgomp|libomp/.test(libraries))
          throw new Error(`${name} unexpectedly links a parallel native runtime:\n${libraries}`);
      }
    }
    // Exercise the actual Criterion/paired target, not just its library adapters.
    const worker = await buildBenchmark(tree, target, "smoke", features, { profile: "dev" });
    const check = join(out, "worker-check");
    await mkdir(check);
    const planPath = join(check, "plan.json");
    const plan = await loadPlan("continuous", {
      types: catalog.types.map((entry) => entry.id),
      distributions: ["random", "equal"],
      lengths: [32],
    });
    const available = await command(executable, ["list"], { cwd: tree, capture: true });
    for (const line of available.split("\n").filter(Boolean)) {
      const id = line.split("\t")[0];
      if (catalog.algorithms.find((entry) => entry.id === id)?.adapter === "revision") continue;
      await writeFile(planPath, JSON.stringify({ ...plan, algorithms: [id] }));
      const env: NodeJS.ProcessEnv = { TESSERA_BENCH_PLAN: planPath };
      inventorySchema.parse(
        JSON.parse(
          await command(worker.executable, [], {
            capture: true,
            env: { ...env, TESSERA_BENCH_LIST: "1" },
          }),
        ),
      );
      if (id === "glidesort-sqrt") {
        const memoryPlan = join(check, "memory-plan.json");
        await writeFile(memoryPlan, JSON.stringify({ ...plan, algorithms: ["tessera"] }));
        env.TESSERA_BENCH_SCRATCH = join(check, "memory");
        await command(join(dirname(executable), "memory"), [
          "--plan",
          memoryPlan,
          "--out",
          env.TESSERA_BENCH_SCRATCH,
        ]);
      }
      await command(worker.executable, ["--test"], { env });
    }
    await command(executable, ["validate", "--suite", "smoke"], { cwd: tree });
    await command(
      executable,
      ["run", "--suite", "smoke", "--group", "u64/random", "--sample-ms", "1", "--out", out],
      { cwd: tree },
    );
    await writeReport([out], true);
  });
}

export interface ReleaseOptions {
  root: string;
  out: string;
  plan: Plan;
  cpu?: string;
  group?: string;
  sampleMs: number;
  blocks: number;
  memoryMib: number;
  revisions?: Revisions;
}

/** Retain the executable pair, then measure verified batches and report one experiment. */
export async function compareAlgorithms(options: ReleaseOptions) {
  pairedBudget(options.blocks);
  await mkdir(options.out, { recursive: false });
  return withSessionLock(options.out, async () => {
    await prepareComparison(options);
    return measureComparison(options.out);
  });
}

/** Build the timing/memory executable pair before starting any measurement. */
async function prepareComparison(options: ReleaseOptions) {
  const { root, out, cpu, group, sampleMs, memoryMib, blocks } = options;
  // Controls belong to this measurement operation, not the shared selection parser.
  const plan = parsePlan({
    ...options.plan,
    algorithms: [...new Set(["tessera", "std-stable", ...options.plan.algorithms])],
  });
  if (cpu) await command("taskset", ["-c", cpu, "true"]);
  const native = plan.algorithms.some((id) =>
    catalog.algorithms.some((entry) => entry.id === id && entry.adapter === "native"),
  );
  const planPath = join(out, "plan.json");
  await writeFile(planPath, `${JSON.stringify(plan, null, 2)}\n`);
  const build = await withComparisonWorkspace(root, native, async ({ tree, target }) => {
    const features = [
      "compare",
      ...(native ? ["compare-native"] : []),
      ...(plan.algorithms.some((id) =>
        catalog.algorithms.some((entry) => entry.id === id && entry.threads),
      )
        ? ["rayon"]
        : []),
    ];
    if (options.revisions) await prepareRevisions(root, tree, options.revisions, features);
    diagnostic("Building comparison workers");
    const { executable } = await buildBenchmark(tree, target, "compare", features);
    return retainBuild(out, executable);
  });
  const session: Session = {
    schema: 2,
    platform: process.platform,
    planSha256: await fileHash(planPath),
    cpu,
    group,
    sampleMs,
    blocks,
    memoryMib,
    build,
  };
  await writeFile(join(out, "session.json"), `${JSON.stringify(session, null, 2)}\n`);
  diagnostic(`Prepared; resume with: bun run tools compare resume ${out}`);
}

/** Resume with the retained selection, sampling settings and exact executable pair. */
export async function resumeComparison(out: string) {
  return withSessionLock(out, () => measureComparison(out));
}

/** Check immutable inputs, then let the native journal resume verified groups. */
async function measureComparison(out: string) {
  const session = await loadSession(out);
  const { cpu, group, sampleMs, memoryMib } = session;
  const selection = [
    "--plan",
    join(out, "plan.json"),
    "--out",
    out,
    ...(group ? ["--group", group] : []),
    ...(session.blocks === undefined ? [] : ["--blocks", String(session.blocks)]),
  ];
  const binary = executables(out).compare;
  const execute = (args: string[]) =>
    command(cpu ? "taskset" : binary, cpu ? ["-c", cpu, binary, ...args] : args, { cwd: out });
  diagnostic("Measuring; completed groups are retained");
  await execute([
    "run",
    ...selection,
    "--sample-ms",
    String(sampleMs),
    "--memory-mib",
    String(memoryMib),
  ]);
  diagnostic("Generating the offline report");
  return writeReport([out], Boolean(group));
}
