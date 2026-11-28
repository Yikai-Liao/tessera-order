// SPDX-License-Identifier: BSD-2-Clause

import { mkdir, rm, symlink, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { command, diagnostic } from "../tools/process.ts";
import { catalog, parseColumnId, referenceColumn } from "./catalog.ts";
import { loadPlan, type Plan, parsePlan, planFeatures, planOperations } from "./plan.ts";
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

const goImplementations = catalog.implementations
  .filter((entry) => entry.adapter === "go")
  .map((entry) => entry.id);

/** Default implementation comparisons extend the existing built-in selections. */
export function includeGoOpponents(plan: Plan): Plan {
  return parsePlan({
    ...plan,
    implementations: [...new Set([...plan.implementations, ...goImplementations])],
  });
}

export async function buildGoWorker(root: string, workspace: Workspace) {
  const { tree, directory } = workspace;
  if (root !== tree)
    for (const path of ["bench/go", "go/tessera"]) {
      await rm(join(tree, path), { recursive: true, force: true });
      await copySnapshot(join(root, path), join(tree, path), root);
    }
  const binary = join(directory, process.platform === "win32" ? "go-worker.exe" : "go-worker");
  await command(
    "go",
    ["build", "-mod=readonly", "-trimpath", "-buildvcs=false", "-o", binary, "."],
    {
      cwd: join(tree, "bench/go"),
      env: { GOTOOLCHAIN: "go1.27.4", GOWORK: "off", GOEXPERIMENT: "simd", CGO_ENABLED: "0" },
    },
  );
  return binary;
}

async function validateGoCases(
  root: string,
  out: string,
  workspace: Workspace,
  executable: string,
) {
  const worker = await buildGoWorker(root, workspace);
  const operations = [
    { kind: "sort" },
    { kind: "argsort" },
    { kind: "select_nth", index: 16 },
    { kind: "partition-buckets", bucket_count: 17 },
  ] as const;
  for (const operation of operations) {
    const plan = includeGoOpponents(
      await loadPlan("smoke", {
        operation: operation.kind,
        index: "index" in operation ? operation.index : undefined,
        bucketCount: "bucket_count" in operation ? operation.bucket_count : undefined,
        types: catalog.types.map((entry) => entry.id),
        distributions: ["random", "d16"],
        lengths: [32],
        executions:
          operation.kind === "sort" || operation.kind === "argsort"
            ? ["serial", "t1", "t2"]
            : ["serial"],
      }),
    );
    const planPath = join(out, `${operation.kind}.json`);
    await writeFile(planPath, JSON.stringify(plan));
    await command(executable, [
      "validate",
      "--plan",
      planPath,
      "--go-worker",
      worker,
      "--out",
      out,
    ]);
  }
}

/** Reproduce CI's harness tests, smoke validation, tiny timing run and offline report. */
export async function checkImplementations(root: string, out: string, native: boolean, go = false) {
  if (go) native = false;
  if (native && process.platform !== "linux")
    throw new Error("Native linkage checks require Linux and ldd");
  await mkdir(out, { recursive: false });
  await withComparisonWorkspace(root, native, async (workspace) => {
    const { tree, target } = workspace;
    const features = [
      ...(go ? [] : ["rayon", "operations"]),
      "parallel-operations",
      ...(native ? ["compare-native"] : []),
    ];
    if (!go) {
      const vectors = "conformance/vectors/benchmark-inputs.json";
      await copySnapshot(join(root, vectors), join(tree, vectors), root);
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
    }
    const { executable } = await buildBenchmark(tree, target, "compare", features, {
      profile: "dev",
    });
    if (go) return validateGoCases(root, out, workspace, executable);
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
      operation: "sort",
      types: catalog.types.map((entry) => entry.id),
      distributions: ["random", "equal"],
      lengths: [32],
    });
    const available = await command(executable, ["list"], { cwd: tree, capture: true });
    const columns = available
      .split("\n")
      .filter(Boolean)
      .map((line) => parseColumnId(line.split("\t")[0]).implementation);
    for (const id of new Set(columns)) {
      if (catalog.implementations.find((entry) => entry.id === id)?.adapter === "revision")
        continue;
      await writeFile(planPath, JSON.stringify({ ...plan, implementations: [id] }));
      const env: NodeJS.ProcessEnv = { TESSERA_BENCH_PLAN: planPath };
      const listed = await command(worker.executable, [], {
        capture: true,
        env: { ...env, TESSERA_BENCH_LIST: "1" },
      });
      if (!listed.trim()) throw new Error(`Shared worker lists no cases for ${id}`);
      if (id === "glidesort-sqrt") {
        const memoryPlan = join(check, "memory-plan.json");
        await writeFile(memoryPlan, JSON.stringify({ ...plan, implementations: ["tessera"] }));
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
    await command(executable, ["validate", "--plan", "smoke"], { cwd: tree });
    await command(
      executable,
      ["run", "--plan", "smoke", "--group", "sort/u64/random", "--sample-ms", "1", "--out", out],
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
export async function compareImplementations(options: ReleaseOptions) {
  pairedBudget(options.blocks);
  // A session's report sections and denominator belong to one operation.
  if (planOperations(options.plan).length !== 1)
    throw new Error(
      "An implementation comparison measures one operation; select it with --operation",
    );
  if (options.revisions && planOperations(options.plan).some((op) => op.kind !== "sort"))
    throw new Error(
      "Linked revisions support full sorting only; use compare implementations for other operations",
    );
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
    implementations: [
      ...new Set([
        "tessera",
        ...planOperations(options.plan).map(
          (operation) => referenceColumn(operation, "serial")!.implementation,
        ),
        ...options.plan.implementations,
      ]),
    ],
  });
  if (cpu) await command("taskset", ["-c", cpu, "true"]);
  const native = plan.implementations.some((id) =>
    catalog.implementations.some((entry) => entry.id === id && entry.adapter === "native"),
  );
  const planPath = join(out, "plan.json");
  await writeFile(planPath, `${JSON.stringify(plan, null, 2)}\n`);
  const build = await withComparisonWorkspace(root, native, async (workspace) => {
    const { tree, target } = workspace;
    const features = [...planFeatures(plan), ...(native ? ["compare-native"] : [])];
    if (options.revisions) await prepareRevisions(root, tree, options.revisions, features);
    diagnostic("Building comparison workers");
    const { executable } = await buildBenchmark(tree, target, "compare", features);
    const goWorker = plan.implementations.some((id) => goImplementations.includes(id))
      ? await buildGoWorker(root, workspace)
      : undefined;
    return retainBuild(out, executable, goWorker);
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
  const paths = executables(out);
  const binary = paths.compare;
  const execute = (args: string[]) =>
    command(cpu ? "taskset" : binary, cpu ? ["-c", cpu, binary, ...args] : args, { cwd: out });
  diagnostic("Measuring; completed groups are retained");
  await execute([
    "run",
    ...selection,
    ...(session.build.goSha256 ? ["--go-worker", paths.go] : []),
    "--sample-ms",
    String(sampleMs),
    "--memory-mib",
    String(memoryMib),
  ]);
  diagnostic("Generating the offline report");
  return writeReport([out], Boolean(group));
}
