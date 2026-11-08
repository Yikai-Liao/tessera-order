// SPDX-License-Identifier: BSD-2-Clause

import { mkdir, symlink, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { command } from "../tools/process.ts";
import { catalog } from "./catalog.ts";
import type { Plan } from "./plan.ts";
import { writeReport } from "./report/index.ts";
import { fetchNative } from "./rust/native/fetch.ts";
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
}

/** Build, validate and measure a comparison session, then write its offline report. */
export async function compareAlgorithms(options: ReleaseOptions) {
  const { root, out, plan, simd, cpu, group } = options;
  const native = plan.algorithms.some((id) =>
    catalog.algorithms.some((entry) => entry.id === id && entry.adapter === "native"),
  );
  await mkdir(out, { recursive: false });
  const planPath = join(out, "plan.json");
  await writeFile(planPath, `${JSON.stringify(plan, null, 2)}\n`);
  await withComparisonWorkspace(root, native, async ({ tree, target }) => {
    for (const portable of simd ? [false, true] : [false]) {
      const features = [
        "compare",
        ...(native && !portable ? ["compare-native"] : []),
        ...(portable ? ["portable-simd"] : []),
      ];
      const { executable: binary } = await buildBenchmark(tree, target, "compare", features);
      const selection = ["--plan", planPath, ...(group ? ["--group", group] : [])];
      await command(binary, ["validate", ...selection], { cwd: tree });
      const args = ["run", ...selection, "--out", out];
      await command(cpu ? "taskset" : binary, cpu ? ["-c", cpu, binary, ...args] : args, {
        cwd: tree,
      });
    }
    await writeReport([out], Boolean(group));
  });
}
