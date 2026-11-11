// SPDX-License-Identifier: BSD-2-Clause

import { createHash } from "node:crypto";
import { cp, mkdir, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { arch, cpus, platform, release } from "node:os";
import { basename, dirname, isAbsolute, join, relative, sep } from "node:path";
import { parse, stringify } from "smol-toml";
import { command, diagnostic } from "../../tools/process.ts";
import { type Plan, parsePlan, selectCaseIds } from "../plan.ts";
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
import {
  engineOrderingSymbols,
  type LayoutControl,
  type LayoutRun,
  parseLayoutSeeds,
  validateLayoutBuilds,
} from "./layout.ts";
import { type LocalRun, shardCases, summarizeLocal } from "./model.ts";
import { renderLocalReport } from "./report.ts";
import { measureSamples } from "./samples.ts";

interface LocalOptions {
  root: string;
  baseline: string;
  out: string;
  filter?: string;
  plan: Plan;
  cpu?: string;
  layoutSeeds?: number[];
  layoutControl?: LayoutControl;
  shard?: { index: number; count: number };
  prepareOnly?: boolean;
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
    "PAIRED",
  ])
    if (`TESSERA_BENCH_${name}` in process.env)
      throw new Error(`Unset TESSERA_BENCH_${name} before starting an experiment`);
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
    baseline: join(directory, `baseline-${layout?.baselineSeed ?? "default"}`, "worker"),
    current: join(directory, `current-${layout?.currentSeed ?? "default"}`, "worker"),
  };
  for (const binary of Object.values(binaries)) await mkdir(dirname(binary), { recursive: true });
  const first = await build("baseline", layout?.baselineSeed);
  if (layout) {
    if (first.provenance.target !== "x86_64-unknown-linux-gnu")
      throw new Error("Layout experiments require the x86_64-unknown-linux-gnu target");
    await cp(map, join(layout.out, `layout-${layout.baselineSeed}-baseline.map`));
  }
  await cp(first.executable, binaries.baseline);
  await rm(crate, { recursive: true });
  await copySnapshot(saved, crate, directory);
  // A tooling-only change has identical library inputs; rebuilding cannot add evidence.
  // Layout experiments still retain their per-role link artifacts.
  const second =
    source.current === source.baseline && !layout
      ? first
      : await build("current", layout?.currentSeed);
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
        env: {
          TESSERA_BENCH_LIST: "1",
          TESSERA_BENCH_PLAN: planPath,
          TESSERA_BENCH_PARALLEL: parallel ? "4" : undefined,
        },
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

/** Select exactly four allowed CPUs for parallel calls and retain their actual mask. */
export async function comparisonCpus(parallel: boolean, requested?: string) {
  if (!parallel) return requested;
  if (platform() !== "linux") throw new Error("Four-thread comparisons require Linux");
  const allowed =
    (await readFile("/proc/self/status", "utf8")).match(/^Cpus_allowed_list:\s*(.+)$/m)?.[1] ?? "";
  const selected = new Set<number>();
  for (const range of (requested ?? allowed).split(",")) {
    const [first, last = first] = range.split("-").map(Number);
    for (let cpu = first; cpu <= last && selected.size < 4; cpu++) selected.add(cpu);
  }
  const permits = (cpu: number) =>
    allowed.split(",").some((range) => {
      const [first, last = first] = range.split("-").map(Number);
      return cpu >= first && cpu <= last;
    });
  if (selected.size !== 4 || [...selected].some((cpu) => !permits(cpu)))
    throw new Error("Four-thread comparisons require four available CPUs");
  return [...selected].join(",");
}

/** Record the host that actually executes samples, separately from build provenance. */
function measurementHost(cpu?: string): LocalRun["host"] {
  return {
    platform: platform(),
    release: release(),
    arch: arch(),
    cpu: cpus()[0]?.model ?? "unknown",
    affinity: cpu ?? "inherited",
    aslr: platform() === "linux" ? "disabled" : "uncontrolled",
  };
}

/** Execute a retained revision pair without snapshots or compilation.
 *
 * Preparation is immutable; each shard gets its own result directory and host
 * observations. Executable hashes and source/selection compatibility are checked
 * before any retained code runs. Sampling and reports use the ordinary runner.
 */
export async function comparePrepared(options: {
  root: string;
  prepared: string;
  out: string;
  cpu?: string;
  shard?: { index: number; count: number };
}) {
  const { root, prepared, out } = options;
  assertWorkerEnvironment();
  if (platform() === "linux") await command("setarch", ["-R", "true"]);
  const plan = parsePlan(JSON.parse(await readFile(join(prepared, "plan.json"), "utf8")));
  if (plan.harness !== "smoke")
    throw new Error("Prepared comparisons require a smoke harness plan");
  const run = JSON.parse(await readFile(join(prepared, "local.json"), "utf8")) as LocalRun;
  if (
    run.schema !== 1 ||
    run.protocol !== "paired-samples-v2" ||
    run.shard ||
    run.samples ||
    run.sampleCounts ||
    run.blocks.length ||
    !run.cases.length ||
    !run.builds ||
    !run.binaryPaths ||
    new Set(run.cases).size !== run.cases.length
  )
    throw new Error("Expected an unmeasured prepared revision pair");
  const catalogHash = createHash("sha256")
    .update(await readFile(join(root, "bench/catalog.toml")))
    .digest("hex");
  if (
    run.source.harness !== (await sourceHash(join(root, "bench/rust"))) ||
    run.source.catalog !== catalogHash
  )
    throw new Error("Prepared pair requires its original harness and catalog");
  const cpu = await comparisonCpus(Boolean(plan.parallel?.length), options.cpu);
  const binaries = { baseline: "", current: "" };
  const preparedRoot = await realpath(prepared);
  await mkdir(out, { recursive: false });
  await writeFile(join(out, "plan.json"), `${JSON.stringify(plan, null, 2)}\n`);
  for (const role of ["baseline", "current"] as const) {
    const path = run.binaryPaths[role];
    if (typeof path !== "string" || !path || isAbsolute(path) || path.split(/[\\/]/).includes(".."))
      throw new Error("Prepared executable path must stay inside its directory");
    const input = await realpath(join(preparedRoot, path));
    const within = relative(preparedRoot, input);
    if (within.split(sep)[0] === ".." || isAbsolute(within))
      throw new Error("Prepared executable path escapes its directory");
    const bytes = await readFile(input);
    if (createHash("sha256").update(bytes).digest("hex") !== run.binaries[role])
      throw new Error("Prepared executable hash differs from build provenance");
    binaries[role] = join(out, path);
    await mkdir(dirname(binaries[role]), { recursive: true });
    await writeFile(binaries[role], bytes, { mode: 0o755 });
  }
  if (basename(binaries.baseline) !== basename(binaries.current))
    throw new Error("Prepared executables must have identical basenames");
  const allCases = await selectedCases(
    binaries,
    join(out, "plan.json"),
    undefined,
    root,
    Boolean(plan.parallel?.length),
  );
  if (JSON.stringify(allCases) !== JSON.stringify(run.cases))
    throw new Error("Prepared executable inventory differs from its recorded cases");
  run.host = measurementHost(cpu);
  run.cases = options.shard
    ? shardCases(allCases, options.shard.index, options.shard.count)
    : allCases;
  if (options.shard) run.shard = { ...options.shard, allCases };
  const save = () => writeFile(join(out, "local.json"), `${JSON.stringify(run, null, 2)}\n`);
  await save();
  await measureSamples(run, binaries, join(out, "plan.json"), cpu, save);
  summarizeLocal(run);
  await renderLocalReport(out, run);
}

/** Compare an immutable working-tree snapshot with a commit, retaining only results. */
export async function compareLocal(options: LocalOptions) {
  const { root, out } = options;
  if (
    options.prepareOnly &&
    (options.shard || options.layoutSeeds || options.layoutControl || options.filter)
  )
    throw new Error("Preparation requires a complete non-layout revision pair");
  // Probe before compiling; do not silently publish an uncontrolled ASLR run.
  if (platform() === "linux") await command("setarch", ["-R", "true"]);
  const cpu = await comparisonCpus(Boolean(options.plan.parallel?.length), options.cpu);
  options = { ...options, cpu };
  if (options.layoutControl && !options.layoutSeeds)
    throw new Error("Layout controls require --layout-seeds");
  assertWorkerEnvironment();
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
        [...(options.plan.parallel?.length ? ["rayon"] : [])],
        seed === undefined ? undefined : { baselineSeed: seed, currentSeed: seed, out },
        options.layoutControl,
      );
      // Measure the retained copies, so downloads contain the exact executed files.
      const binaryPaths = { baseline: "", current: "" };
      for (const role of ["baseline", "current"] as const) {
        const retained = join(out, "binaries", relative(workspace.directory, binaries[role]));
        await mkdir(dirname(retained), { recursive: true });
        await cp(binaries[role], retained);
        binaries[role] = retained;
        binaryPaths[role] = relative(out, retained);
      }
      const allCases = await selectedCases(
        binaries,
        planPath,
        options.filter,
        workspace.tree,
        Boolean(options.plan.parallel?.length),
      );
      const run: LocalRun = {
        schema: 1,
        protocol: "paired-samples-v2",
        baseline,
        head,
        dirty,
        source,
        binaries: hashes,
        binaryPaths,
        builds,
        rustc: builds.current.rustc,
        flags: builds.current.cargo_encoded_rustflags,
        configuration: "portable-simd",
        host: measurementHost(cpu),
        cases: options.shard
          ? shardCases(allCases, options.shard.index, options.shard.count)
          : allCases,
        ...(options.shard ? { shard: { ...options.shard, allCases } } : {}),
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
    if (options.prepareOnly) return;
    for (const [index, { seed, run }] of layouts.entries()) {
      if (linker) diagnostic(`Layout seed ${seed}`);
      await measureSamples(run, executables[index], planPath, cpu, save);
      summarizeLocal(run);
    }
    await renderLocalReport(out, result);
  });
}
