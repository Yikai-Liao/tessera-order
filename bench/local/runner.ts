// SPDX-License-Identifier: BSD-2-Clause

import { createHash } from "node:crypto";
import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { arch, cpus, platform, release } from "node:os";
import { join } from "node:path";
import { parse, stringify } from "smol-toml";
import { command, diagnostic } from "../../tools/process.ts";
import { type Plan, selectCaseIds } from "../plan.ts";
import {
  buildBenchmark,
  copyLibraryRevision,
  copySnapshot,
  sourceHash,
  type Workspace,
  withWorkspace,
} from "../workspace.ts";
import { readEstimates } from "./criterion.ts";
import { type LayoutRun, parseLayoutSeeds, validateLayoutBuilds } from "./layout.ts";
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
  layoutSeeds?: number[];
}

/** Complete only unused optional metadata for pre-parallel serial baselines. */
export function serialBaselineManifest(baseline: string, current: string) {
  const manifest = parse(baseline);
  const features = manifest.features as Record<string, unknown>;
  if ("rayon" in features) return baseline;
  const dependencies = manifest.dependencies as Record<string, unknown>;
  if ("rayon" in dependencies)
    throw new Error("Historical Rayon dependencies require explicit review");
  const reference = parse(current);
  const dependency = (reference.dependencies as Record<string, { optional?: boolean }>).rayon;
  if (!dependency?.optional) throw new Error("The serial baseline shim requires optional Rayon");
  features.rayon = (reference.features as Record<string, unknown>).rayon;
  dependencies.rayon = dependency;
  return stringify(manifest);
}

/** Compile both revisions with one harness, lockfile, toolchain and build directory. */
export async function buildRevisions(
  root: string,
  workspace: Workspace,
  baseline: string,
  features: string[],
  layout?: { baselineSeed: number; currentSeed: number; out: string },
) {
  const { tree, directory, target } = workspace;
  const crate = join(tree, "rust/tessera-sort");
  const saved = join(directory, "current-source");
  await copySnapshot(crate, saved, tree);
  const source = {
    current: await sourceHash(crate),
    baseline: "",
    harness: await sourceHash(join(tree, "bench/rust")),
    catalog: createHash("sha256")
      .update(await readFile(join(tree, "bench/catalog.toml")))
      .digest("hex"),
  };
  await rm(crate, { recursive: true });
  await copyLibraryRevision(root, baseline, crate);
  // Cargo validates inactive forwarding and locks optional dependency edges.
  // Keep the shared harness/lockfile; this metadata never enables a parallel API.
  const manifest = join(crate, "Cargo.toml");
  await writeFile(
    manifest,
    serialBaselineManifest(
      await readFile(manifest, "utf8"),
      await readFile(join(saved, "Cargo.toml"), "utf8"),
    ),
  );
  source.baseline = await sourceHash(crate);
  const map = join(directory, "layout.map");
  // Keep the map path out of -Wl's comma-separated argument list.
  const rustcArgs = (seed: number | undefined) =>
    seed === undefined
      ? undefined
      : [
          "-C",
          "linker=cc",
          "-C",
          "linker-features=-lld",
          "-C",
          "link-arg=-fuse-ld=lld",
          "-C",
          `link-arg=-Wl,--shuffle-sections=.text.*=${seed}`,
          "-C",
          "link-arg=-Xlinker",
          "-C",
          `link-arg=-Map=${map}`,
        ];
  const binaries = {
    baseline: join(directory, `baseline-${layout?.baselineSeed ?? "default"}`),
    current: join(directory, `current-${layout?.currentSeed ?? "default"}`),
  };
  const first = await buildBenchmark(tree, target, "smoke", features, {
    rustcArgs: rustcArgs(layout?.baselineSeed),
  });
  if (layout) {
    if (first.provenance.target !== "x86_64-unknown-linux-gnu")
      throw new Error("Layout experiments require the x86_64-unknown-linux-gnu target");
    await cp(map, join(layout.out, `layout-${layout.baselineSeed}-baseline.map`));
  }
  await cp(first.executable, binaries.baseline);
  await rm(crate, { recursive: true });
  await copySnapshot(saved, crate, directory);
  const second = await buildBenchmark(tree, target, "smoke", features, {
    rustcArgs: rustcArgs(layout?.currentSeed),
  });
  if (layout) await cp(map, join(layout.out, `layout-${layout.currentSeed}-current.map`));
  await cp(second.executable, binaries.current);
  for (const field of [
    "rustc",
    "cargo_encoded_rustflags",
    "target",
    "profile",
    "opt_level",
    "debug",
  ] as const)
    if (first.provenance[field] !== second.provenance[field])
      throw new Error(`Build settings differ between revisions: ${field}`);
  const hash = async (binary: string) =>
    createHash("sha256")
      .update(await readFile(binary))
      .digest("hex");
  return {
    source,
    binaries,
    builds: { baseline: first.provenance, current: second.provenance },
    hashes: { baseline: await hash(binaries.baseline), current: await hash(binaries.current) },
  };
}

/** Select the same timed IDs in both executables before starting measurements. */
export async function selectedCases(
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
export async function measureBlock(
  cases: string[],
  binaries: { baseline: string; current: string },
  workspace: Workspace,
  options: { block: number; planPath: string; cpu?: string; timeDivisor?: number },
): Promise<LocalBlock> {
  const values = {} as LocalBlock;
  values.elapsedMs = { baseline: 0, control: 0, current: 0 };
  const roles = ["baseline", "control", "current"] as const;
  const orders = [
    [0, 1, 2],
    [2, 1, 0],
    [1, 2, 0],
    [0, 2, 1],
    [2, 0, 1],
    [1, 0, 2],
  ];
  const groups = [...new Set(cases.map((id) => id.slice(0, id.lastIndexOf("/"))))].join("\n");
  for (const index of orders[options.block % orders.length]) {
    const started = performance.now();
    const role = roles[index],
      binary = role === "current" ? binaries.current : binaries.baseline;
    const criterion = join(workspace.directory, "criterion");
    await rm(criterion, { recursive: true, force: true });
    diagnostic(`Block ${options.block + 1}/9: ${role}`);
    const pattern = cases.map((id) => id.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|");
    const args = ["--bench", `^(?:${pattern})$`];
    const { cpu } = options;
    await command(cpu ? "taskset" : binary, cpu ? ["-c", cpu, binary, ...args] : args, {
      cwd: workspace.tree,
      env: {
        CRITERION_HOME: criterion,
        TESSERA_BENCH_GROUPS: groups,
        TESSERA_BENCH_PLAN: options.planPath,
        TESSERA_BENCH_TIME_DIVISOR: String(options.timeDivisor ?? 1),
      },
    });
    values[role] = await readEstimates(criterion);
    values.elapsedMs[role] = performance.now() - started;
  }
  return values;
}

/** Identify the compiler driver and the LLD it will select for local layout builds. */
export async function readLayoutLinker(): Promise<LayoutRun["linker"]> {
  if (platform() !== "linux" || arch() !== "x64")
    throw new Error("Layout experiments require Linux x86-64, cc and LLD");
  return {
    driver: await command("cc", ["--version"], { capture: true }),
    version: await command(
      await command("cc", ["-print-prog-name=ld.lld"], { capture: true }),
      ["--version"],
      { capture: true },
    ),
  };
}

/** Compare an immutable working-tree snapshot with a commit, retaining only results. */
export async function compareLocal(options: LocalOptions) {
  const { root, out, simd, cpu } = options;
  if (
    [
      "TESSERA_BENCH_LIST",
      "TESSERA_BENCH_GROUPS",
      "TESSERA_BENCH_PLAN",
      "TESSERA_BENCH_IDS",
      "TESSERA_BENCH_PARALLEL",
    ].some((name) => name in process.env)
  )
    throw new Error("Unset worker-selection variables before starting a local comparison");
  let linker: LayoutRun["linker"] | undefined;
  if (options.layoutSeeds) {
    parseLayoutSeeds(options.layoutSeeds.join(","));
    linker = await readLayoutLinker();
  }
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
    const layouts: LayoutRun["layouts"] = [];
    const executables: { baseline: string; current: string }[] = [];
    for (const seed of options.layoutSeeds ?? [undefined]) {
      diagnostic(`Building revision pair${seed === undefined ? "" : ` for layout seed ${seed}`}`);
      const { binaries, source, hashes, builds } = await buildRevisions(
        root,
        workspace,
        baseline,
        simd ? ["portable-simd"] : [],
        seed === undefined ? undefined : { baselineSeed: seed, currentSeed: seed, out },
      );
      const run: LocalRun = {
        schema: 1,
        baseline,
        head,
        dirty,
        source,
        binaries: hashes,
        builds,
        rustc: builds.current.rustc,
        flags: builds.current.cargo_encoded_rustflags,
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
      if (layouts.length && run.cases.join("\n") !== layouts[0].run.cases.join("\n"))
        throw new Error("Layout runs must select identical cases");
      layouts.push({ seed: seed ?? 0, run });
      executables.push(binaries);
    }
    const result: LocalRun | LayoutRun = linker ? { schema: 2, linker, layouts } : layouts[0].run;
    if (result.schema === 2) validateLayoutBuilds(result);
    const save = () => writeFile(join(out, "local.json"), `${JSON.stringify(result, null, 2)}\n`);
    await save();
    for (let block = 0; block < 9; block++) {
      // Rotate layouts between blocks so one seed does not own one time interval.
      for (let offset = 0; offset < layouts.length; offset++) {
        const index = (block + offset) % layouts.length;
        const { seed, run } = layouts[index];
        if (linker) diagnostic(`Layout seed ${seed}`);
        run.blocks.push(
          await measureBlock(run.cases, executables[index], workspace, { block, planPath, cpu }),
        );
        if (run.blocks.length === 9) summarizeLocal(run);
        await save();
      }
    }
    await renderLocalReport(out, result);
  });
}
