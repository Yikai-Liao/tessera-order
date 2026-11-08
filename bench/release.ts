// SPDX-License-Identifier: BSD-2-Clause

import { mkdir, symlink, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { catalog } from "./catalog.ts";
import type { Plan } from "./plan.ts";
import { command } from "./process.ts";
import { writeReport } from "./report/index.ts";
import { withWorkspace } from "./workspace.ts";

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
  if (native) await command("python3", ["bench/fetch.py"], { cwd: root });
  await withWorkspace(root, async ({ tree, target }) => {
    if (native) await symlink(join(root, "bench/vendor"), join(tree, "bench/vendor"), "dir");
    for (const portable of simd ? [false, true] : [false]) {
      const features = [
        "compare",
        ...(native && !portable ? ["compare-native"] : []),
        ...(portable ? ["portable-simd"] : []),
      ];
      await command(
        "cargo",
        [
          "build",
          "--locked",
          "--release",
          "-p",
          "tessera-bench",
          "--features",
          features.join(","),
          "--target-dir",
          target,
        ],
        { cwd: tree },
      );
      const binary = join(
        target,
        "release",
        process.platform === "win32" ? "compare.exe" : "compare",
      );
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
