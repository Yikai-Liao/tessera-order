// SPDX-License-Identifier: BSD-2-Clause

import { mkdir, symlink, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { command, diagnostic } from "../tools/process.ts";
import { catalog } from "./catalog.ts";
import type { Plan } from "./plan.ts";
import { writeReport } from "./report/index.ts";
import { fetchNative } from "./rust/native/fetch.ts";
import {
  type Build,
  executables,
  fileHash,
  isValidated,
  loadSession,
  recordValidation,
  retainBuild,
  type Session,
  withSessionLock,
} from "./session.ts";
import { buildBenchmark, copySnapshot, type Workspace, withWorkspace } from "./workspace.ts";

/** Acquire native sources and own their link inside an isolated build workspace. */
async function withComparisonWorkspace<T>(
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
    const features = ["compare", ...(native ? ["compare-native"] : [])];
    await command(
      "cargo",
      [
        "test",
        "--locked",
        "-p",
        "tessera-bench",
        "--features",
        features.join(","),
        "--lib",
        "--bin",
        "memory",
        "--test",
        "comparison",
        "--target-dir",
        target,
      ],
      { cwd: tree },
    );
    const { executable } = await buildBenchmark(tree, target, "compare", features);
    if (native) {
      for (const name of ["compare", "memory"]) {
        const binary = name === "compare" ? executable : join(dirname(executable), "memory");
        const libraries = await command("ldd", [binary], { capture: true });
        if (/libtbb|libgomp|libomp/.test(libraries))
          throw new Error(`${name} unexpectedly links a parallel native runtime:\n${libraries}`);
      }
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
  simd: boolean;
  cpu?: string;
  group?: string;
  sampleMs: number;
  memoryMib: number;
}

/** Retain immutable builds, then validate, measure and report one experiment. */
export async function compareAlgorithms(options: ReleaseOptions) {
  await mkdir(options.out, { recursive: false });
  await withSessionLock(options.out, async () => {
    await prepareComparison(options);
    await measureComparison(options.out);
  });
}

/** Build every selected configuration before starting any measurement. */
async function prepareComparison(options: ReleaseOptions) {
  const { root, out, plan, simd, cpu, group, sampleMs, memoryMib } = options;
  if (cpu) await command("taskset", ["-c", cpu, "true"]);
  const native = plan.algorithms.some((id) =>
    catalog.algorithms.some((entry) => entry.id === id && entry.adapter === "native"),
  );
  const planPath = join(out, "plan.json");
  await writeFile(planPath, `${JSON.stringify(plan, null, 2)}\n`);
  const builds: Build[] = [];
  await withComparisonWorkspace(root, native, async ({ tree, target }) => {
    for (const portable of simd ? [false, true] : [false]) {
      const features = [
        "compare",
        ...(native && !portable ? ["compare-native"] : []),
        ...(plan.algorithms.some((id) =>
          catalog.algorithms.some((entry) => entry.id === id && entry.threads),
        )
          ? ["rayon"]
          : []),
        ...(portable ? ["portable-simd"] : []),
      ];
      const name = portable ? "portable-simd" : "default";
      diagnostic(`Building ${name}`);
      const { executable } = await buildBenchmark(tree, target, "compare", features);
      builds.push(await retainBuild(out, name, executable));
    }
  });
  const session: Session = {
    schema: 1,
    platform: process.platform,
    planSha256: await fileHash(planPath),
    cpu,
    group,
    sampleMs,
    memoryMib,
    builds,
  };
  await writeFile(join(out, "session.json"), `${JSON.stringify(session, null, 2)}\n`);
  diagnostic(`Prepared; resume with: pnpm tools compare resume ${out}`);
}

/** Resume with the retained selection, sampling settings and exact executable pairs. */
export async function resumeComparison(out: string) {
  await withSessionLock(out, () => measureComparison(out));
}

/** Validate immutable inputs, then let the native journal resume complete groups. */
async function measureComparison(out: string) {
  const session = await loadSession(out);
  const { cpu, group, sampleMs, memoryMib } = session;
  const selection = [
    "--plan",
    join(out, "plan.json"),
    "--out",
    out,
    ...(group ? ["--group", group] : []),
  ];
  for (const build of session.builds) {
    const binary = executables(out, build.name).compare;
    const execute = (args: string[]) =>
      command(cpu ? "taskset" : binary, cpu ? ["-c", cpu, binary, ...args] : args, { cwd: out });
    if (!(await isValidated(out, build, session))) {
      diagnostic(`Validating ${build.name}`);
      await execute(["validate", ...selection]);
      await recordValidation(out, build, session);
    }
    diagnostic(`Measuring ${build.name}; completed groups are retained`);
    await execute([
      "run",
      ...selection,
      "--sample-ms",
      String(sampleMs),
      "--memory-mib",
      String(memoryMib),
    ]);
  }
  diagnostic("Generating the offline report");
  await writeReport([out], Boolean(group));
}
