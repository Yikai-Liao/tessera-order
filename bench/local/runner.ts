// SPDX-License-Identifier: BSD-2-Clause

import { createHash } from "node:crypto";
import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { arch, cpus, platform, release } from "node:os";
import { join } from "node:path";
import { type Plan, selectCaseIds } from "../plan.ts";
import { command } from "../process.ts";
import { buildSmoke, sourceHash, type Workspace, withWorkspace } from "../workspace.ts";
import { readEstimates } from "./criterion.ts";
import { type LocalBlock, type LocalRun, summarizeLocal } from "./model.ts";
import { renderLocalReport } from "./report.ts";

interface LocalOptions {
  root: string;
  baseline: string;
  out: string;
  filter?: string;
  plan: Plan;
  simd: boolean;
  cpu?: string;
}

/** Compile both revisions with one harness, lockfile, toolchain and build directory. */
async function buildRevisions(root: string, workspace: Workspace, baseline: string, simd: boolean) {
  const { tree, directory, target } = workspace;
  const crate = join(tree, "rust/tessera-sort");
  const saved = join(directory, "current-source");
  await cp(crate, saved, { recursive: true });
  const source = {
    current: await sourceHash(crate),
    baseline: "",
    harness: await sourceHash(join(tree, "bench/rust")),
    catalog: createHash("sha256")
      .update(await readFile(join(tree, "bench/catalog.json")))
      .digest("hex"),
  };
  await rm(crate, { recursive: true });
  const archive = join(directory, "baseline.tar");
  await command(
    "git",
    ["archive", "--format=tar", `--output=${archive}`, baseline, "rust/tessera-sort"],
    { cwd: root },
  );
  await command("tar", ["-xf", archive, "-C", tree]);
  await rm(archive);
  source.baseline = await sourceHash(crate);
  const binaries = { baseline: join(directory, "baseline"), current: join(directory, "current") };
  await cp(await buildSmoke(tree, target, simd), binaries.baseline);
  await rm(crate, { recursive: true });
  await cp(saved, crate, { recursive: true });
  await cp(await buildSmoke(tree, target, simd), binaries.current);
  const hash = async (binary: string) =>
    createHash("sha256")
      .update(await readFile(binary))
      .digest("hex");
  return {
    source,
    binaries,
    hashes: { baseline: await hash(binaries.baseline), current: await hash(binaries.current) },
  };
}

/** Select the same timed IDs in both executables before starting measurements. */
async function selectedCases(
  binaries: { baseline: string; current: string },
  planPath: string,
  pattern: string | undefined,
  cwd: string,
) {
  const list = async (binary: string) =>
    (
      await command(binary, ["--bench"], {
        cwd,
        capture: true,
        env: { TESSERA_BENCH_LIST: "1", TESSERA_BENCH_PLAN: planPath },
      })
    )
      .split("\n")
      .filter(Boolean)
      .sort();
  const cases = selectCaseIds(await list(binaries.current), pattern);
  if (
    !cases.length ||
    new Set(cases).size !== cases.length ||
    cases.join("\n") !== selectCaseIds(await list(binaries.baseline), pattern).join("\n")
  )
    throw new Error("Selected case IDs must be nonempty and identical in both builds");
  return cases;
}

/** Measure one process-level block, cycling through the six possible orders. */
async function measureBlock(
  run: LocalRun,
  binaries: { baseline: string; current: string },
  workspace: Workspace,
  options: LocalOptions,
): Promise<LocalBlock> {
  const values = {} as LocalBlock;
  const roles = ["baseline", "control", "current"] as const;
  const orders = [
    [0, 1, 2],
    [2, 1, 0],
    [1, 2, 0],
    [0, 2, 1],
    [2, 0, 1],
    [1, 0, 2],
  ];
  const groups = [...new Set(run.cases.map((id) => id.slice(0, id.lastIndexOf("/"))))].join("\n");
  for (const index of orders[run.blocks.length % orders.length]) {
    const role = roles[index],
      binary = role === "current" ? binaries.current : binaries.baseline;
    const criterion = join(workspace.directory, "criterion");
    await rm(criterion, { recursive: true, force: true });
    console.log(`Block ${run.blocks.length + 1}/9: ${role}`);
    const pattern = run.cases.map((id) => id.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|");
    const args = ["--bench", `^(?:${pattern})$`];
    const { cpu } = options;
    await command(cpu ? "taskset" : binary, cpu ? ["-c", cpu, binary, ...args] : args, {
      cwd: workspace.tree,
      env: {
        CRITERION_HOME: criterion,
        TESSERA_BENCH_GROUPS: groups,
        TESSERA_BENCH_PLAN: join(options.out, "plan.json"),
      },
    });
    values[role] = await readEstimates(criterion);
  }
  return values;
}

/** Compare an immutable working-tree snapshot with a commit, retaining only results. */
export async function compareLocal(options: LocalOptions) {
  const { root, out, simd, cpu } = options;
  if (
    ["TESSERA_BENCH_LIST", "TESSERA_BENCH_GROUPS", "TESSERA_BENCH_PLAN"].some(
      (name) => name in process.env,
    )
  )
    throw new Error("Unset worker-selection variables before starting a local comparison");
  const baseline = await command("git", ["rev-parse", "--verify", `${options.baseline}^{commit}`], {
    cwd: root,
    capture: true,
  });
  const head = await command("git", ["rev-parse", "HEAD"], { cwd: root, capture: true });
  const dirty = Boolean(
    await command("git", ["status", "--porcelain"], { cwd: root, capture: true }),
  );
  await mkdir(out, { recursive: false });
  const planPath = join(out, "plan.json");
  await writeFile(planPath, `${JSON.stringify(options.plan, null, 2)}\n`);
  await withWorkspace(root, async (workspace) => {
    const { binaries, source, hashes } = await buildRevisions(root, workspace, baseline, simd);
    const run: LocalRun = {
      schema: 1,
      baseline,
      head,
      dirty,
      source,
      binaries: hashes,
      rustc: await command("rustc", ["-vV"], { cwd: workspace.tree, capture: true }),
      flags: process.env.CARGO_ENCODED_RUSTFLAGS ?? process.env.RUSTFLAGS ?? "",
      configuration: simd ? "portable-simd" : "default",
      host: {
        platform: platform(),
        release: release(),
        arch: arch(),
        cpu: cpus()[0]?.model ?? "unknown",
        affinity: cpu ?? "inherited",
      },
      cases: await selectedCases(binaries, planPath, options.filter, workspace.tree),
      blocks: [],
    };
    for (let block = 0; block < 9; block++) {
      run.blocks.push(await measureBlock(run, binaries, workspace, options));
      if (run.blocks.length === 9) summarizeLocal(run);
      await writeFile(join(out, "local.json"), `${JSON.stringify(run, null, 2)}\n`);
    }
    await renderLocalReport(out, run);
  });
}
