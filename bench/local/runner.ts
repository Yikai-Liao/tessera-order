// SPDX-License-Identifier: BSD-2-Clause

import { createHash } from "node:crypto";
import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { arch, cpus, platform, release } from "node:os";
import { join } from "node:path";
import { parse, stringify } from "smol-toml";
import { command, diagnostic } from "../../tools/process.ts";
import { criterionGroup, parseCriterionId } from "../catalog.ts";
import { type Plan, selectCaseIds } from "../plan.ts";
import {
  ALIGNMENT_RUSTFLAGS,
  buildBenchmark,
  copyLibraryRevision,
  copySnapshot,
  simdFeatures,
  sourceHash,
  type Workspace,
  withWorkspace,
} from "../workspace.ts";
import { readEstimates } from "./criterion.ts";
import {
  engineOrderingSymbols,
  type LayoutControl,
  type LayoutRun,
  parseLayoutSeeds,
  validateLayoutBuilds,
} from "./layout.ts";
import { type LocalBlock, type LocalRun, summarizeLocal } from "./model.ts";
import { renderLocalReport } from "./report.ts";

interface LocalOptions {
  root: string;
  baseline: string;
  out: string;
  filter?: string;
  plan: Plan;
  cpu?: string;
  layoutSeeds?: number[];
  layoutControl?: LayoutControl;
}

/** Reject ambient worker controls that could change a requested experiment. */
export function assertWorkerEnvironment() {
  for (const name of [
    "LIST",
    "GROUPS",
    "PLAN",
    "IDS",
    "PARALLEL",
    "TIME_DIVISOR",
    "STUDY_SETTINGS",
    "FIXED_SORTS",
  ])
    if (`TESSERA_BENCH_${name}` in process.env)
      throw new Error(`Unset TESSERA_BENCH_${name} before starting an experiment`);
}

/** Select the smoke harness's existing four-thread API protocol explicitly. */
export function smokeEnvironment(planPath: string, parallel: boolean): NodeJS.ProcessEnv {
  return {
    TESSERA_BENCH_PLAN: planPath,
    ...(parallel ? { TESSERA_BENCH_PARALLEL: "4" } : {}),
  };
}

/** Complete inactive optional metadata only for pre-parallel serial comparisons. */
export function serialBaselineManifest(baseline: string, current: string, parallel = false) {
  const manifest = parse(baseline);
  const features = manifest.features as Record<string, unknown>;
  if ("rayon" in features) return baseline;
  if (parallel) throw new Error("Parallel revision comparisons require a Rayon-capable baseline");
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
  control?: LayoutControl,
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
  // Only serial baselines may receive the inactive optional dependency shim.
  const manifest = join(crate, "Cargo.toml");
  await writeFile(
    manifest,
    serialBaselineManifest(
      await readFile(manifest, "utf8"),
      await readFile(join(saved, "Cargo.toml"), "utf8"),
      features.includes("rayon"),
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
  const extraFlags =
    control === "cgu1" ? ["-Ccodegen-units=1"] : control === "aligned" ? ALIGNMENT_RUSTFLAGS : [];
  let env: NodeJS.ProcessEnv | undefined;
  const build = async (role: "baseline" | "current", seed: number | undefined) => {
    const selectedFeatures = [
      ...features,
      ...simdFeatures(await readFile(manifest, "utf8")).map((feature) => `tessera-sort/${feature}`),
    ];
    const args = rustcArgs(seed);
    if (extraFlags.length && !env) {
      // Cargo resolves target/build config flags before exposing them to the
      // build script. Read that result before selecting its higher-precedence
      // encoded environment source, which would otherwise discard config flags.
      const reference = await buildBenchmark(tree, target, "smoke", selectedFeatures, {
        rustcArgs: args,
      });
      env = {
        CARGO_ENCODED_RUSTFLAGS: [reference.provenance.cargo_encoded_rustflags, ...extraFlags]
          .filter(Boolean)
          .join("\x1f"),
      };
    }
    let built = await buildBenchmark(tree, target, "smoke", selectedFeatures, {
      rustcArgs: args,
      env,
    });
    if (control === "engine-first") {
      if (!layout || !args) throw new Error("Engine ordering requires layout seeds");
      const views = await Promise.all(
        [[], ["--demangle"]].map((extra) =>
          command(
            "nm",
            ["--defined-only", "--no-sort", "--format=posix", ...extra, built.executable],
            { capture: true },
          ),
        ),
      );
      const symbols = engineOrderingSymbols(views[0], views[1]);
      if (!symbols.length) throw new Error("No out-of-line engine symbols to order");
      const order = join(directory, "engine.order");
      await writeFile(order, `${symbols.join("\n")}\n`);
      await cp(order, join(layout.out, `layout-${seed}-${role}.order`));
      built = await buildBenchmark(tree, target, "smoke", selectedFeatures, {
        env,
        rustcArgs: [
          ...args,
          "-C",
          "link-arg=-Xlinker",
          "-C",
          `link-arg=--symbol-ordering-file=${order}`,
        ],
      });
    }
    return built;
  };
  const binaries = {
    baseline: join(directory, `baseline-${layout?.baselineSeed ?? "default"}`),
    current: join(directory, `current-${layout?.currentSeed ?? "default"}`),
  };
  const first = await build("baseline", layout?.baselineSeed);
  if (layout) {
    if (first.provenance.target !== "x86_64-unknown-linux-gnu")
      throw new Error("Layout experiments require the x86_64-unknown-linux-gnu target");
    await cp(map, join(layout.out, `layout-${layout.baselineSeed}-baseline.map`));
  }
  await cp(first.executable, binaries.baseline);
  await rm(crate, { recursive: true });
  await copySnapshot(saved, crate, directory);
  const second = await build("current", layout?.currentSeed);
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
  parallel = false,
) {
  const list = async (binary: string) =>
    (
      await command(binary, ["--bench"], {
        cwd,
        capture: true,
        env: { TESSERA_BENCH_LIST: "1", ...smokeEnvironment(planPath, parallel) },
      })
    )
      .split("\n")
      .filter(Boolean)
      .sort();
  const current = await list(binaries.current);
  if (parallel && !current.some((id) => parseCriterionId(id).mode === "par-4"))
    throw new Error("Parallel plan did not list any four-thread cases");
  const cases = selectCaseIds(current, pattern);
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
  const groups = [...new Set(run.cases.map(criterionGroup))].join("\n");
  for (const index of orders[run.blocks.length % orders.length]) {
    const role = roles[index],
      binary = role === "current" ? binaries.current : binaries.baseline;
    const criterion = join(workspace.directory, "criterion");
    await rm(criterion, { recursive: true, force: true });
    diagnostic(`Block ${run.blocks.length + 1}/9: ${role}`);
    const pattern = run.cases.map((id) => id.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|");
    const args = ["--bench", `^(?:${pattern})$`];
    const { cpu } = options;
    await command(cpu ? "taskset" : binary, cpu ? ["-c", cpu, binary, ...args] : args, {
      cwd: workspace.tree,
      env: {
        CRITERION_HOME: criterion,
        TESSERA_BENCH_GROUPS: groups,
        ...smokeEnvironment(join(options.out, "plan.json"), Boolean(options.plan.parallel?.length)),
      },
    });
    values[role] = await readEstimates(criterion);
  }
  return values;
}

/** Probe the same inherited/requested affinity that measured child processes use. */
export async function parallelExecution(cpu?: string) {
  if (platform() !== "linux") throw new Error("Parallel revision screens require Linux");
  const probe = `console.log(JSON.stringify({
    allowed: require("node:os").availableParallelism(),
    affinity: require("node:fs").readFileSync("/proc/self/status", "utf8")
      .match(/^Cpus_allowed_list:(.+)$/m)[1].trim()
  }))`;
  const args = ["-e", probe];
  const observed = JSON.parse(
    await command(
      cpu ? "taskset" : process.execPath,
      cpu ? ["-c", cpu, process.execPath, ...args] : args,
      { capture: true },
    ),
  );
  if (
    !Number.isSafeInteger(observed.allowed) ||
    observed.allowed < 4 ||
    typeof observed.affinity !== "string"
  )
    throw new Error("Four allowed CPUs are required for parallel revision screens");
  return {
    threads: 4,
    allowed: observed.allowed as number,
    affinity: observed.affinity as string,
    topology: await command("lscpu", ["-p=CPU,CORE,SOCKET,NODE"], { capture: true }),
  };
}

/** Compare an immutable working-tree snapshot with a commit, retaining only results. */
export async function compareLocal(options: LocalOptions) {
  const { root, out, cpu } = options;
  if (options.layoutControl && !options.layoutSeeds)
    throw new Error("Layout controls require --layout-seeds");
  assertWorkerEnvironment();
  const parallel = options.plan.parallel?.length ? await parallelExecution(cpu) : undefined;
  let linker: LayoutRun["linker"] | undefined;
  if (options.layoutSeeds) {
    parseLayoutSeeds(options.layoutSeeds.join(","));
    if (platform() !== "linux" || arch() !== "x64")
      throw new Error("Layout experiments require Linux x86-64, cc and LLD");
    linker = {
      driver: await command("cc", ["--version"], { capture: true }),
      version: await command(
        await command("cc", ["-print-prog-name=ld.lld"], { capture: true }),
        ["--version"],
        { capture: true },
      ),
    };
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
        options.plan.parallel?.length ? ["rayon"] : [],
        seed === undefined ? undefined : { baselineSeed: seed, currentSeed: seed, out },
        options.layoutControl,
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
        configuration: "portable-simd",
        host: {
          platform: platform(),
          release: release(),
          arch: arch(),
          cpu: cpus()[0]?.model ?? "unknown",
          affinity: cpu ?? "inherited",
          ...(parallel ? { parallel } : {}),
        },
        cases: await selectedCases(
          binaries,
          planPath,
          options.filter,
          workspace.tree,
          Boolean(options.plan.parallel?.length),
        ),
        blocks: [],
      };
      if (layouts.length && run.cases.join("\n") !== layouts[0].run.cases.join("\n"))
        throw new Error("Layout runs must select identical cases");
      layouts.push({ seed: seed ?? 0, run });
      executables.push(binaries);
    }
    const result: LocalRun | LayoutRun = linker
      ? { schema: 2, linker, control: options.layoutControl, layouts }
      : layouts[0].run;
    if (result.schema === 2) validateLayoutBuilds(result);
    const save = () => writeFile(join(out, "local.json"), `${JSON.stringify(result, null, 2)}\n`);
    await save();
    for (let block = 0; block < 9; block++) {
      // Rotate layouts between blocks so one seed does not own one time interval.
      for (let offset = 0; offset < layouts.length; offset++) {
        const index = (block + offset) % layouts.length;
        const { seed, run } = layouts[index];
        if (linker) diagnostic(`Layout seed ${seed}`);
        run.blocks.push(await measureBlock(run, executables[index], workspace, options));
        if (run.blocks.length === 9) summarizeLocal(run);
        await save();
      }
    }
    await renderLocalReport(out, result);
  });
}
