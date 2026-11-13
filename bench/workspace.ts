// SPDX-License-Identifier: BSD-2-Clause

import { createHash } from "node:crypto";
import {
  copyFile,
  cp,
  lstat,
  mkdir,
  mkdtemp,
  readdir,
  readFile,
  realpath,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { homedir } from "node:os";
import { basename, dirname, extname, isAbsolute, join, relative, resolve, sep } from "node:path";
import lockfile from "proper-lockfile";
import { parse } from "smol-toml";
import { command } from "../tools/process.ts";

/** Select the same vector backend on historical opt-in and current required builds.
 * Missing backends fail rather than silently comparing SIMD against scalar code.
 */
export function simdFeatures(manifest: string): string[] {
  const parsed = parse(manifest);
  const features = parsed.features as Record<string, unknown> | undefined;
  if (features && "portable-simd" in features) return ["portable-simd"];
  const dependencies = parsed.dependencies as Record<string, unknown> | undefined;
  const backend = dependencies?.fearless_simd;
  if (
    typeof backend === "string" ||
    (backend && typeof backend === "object" && !("optional" in backend && backend.optional))
  )
    return [];
  throw new Error("Revision has no supported SIMD backend; choose a SIMD-capable baseline");
}

/** Align measured Rust code while retaining later, explicitly configured overrides. */
export const ALIGNMENT_RUSTFLAGS = [
  "-C",
  "llvm-args=-align-loops=64",
  "-C",
  "llvm-args=-align-all-nofallthru-blocks=6",
];

/** Resolve a revision to its full commit SHA. */
export function resolveCommit(root: string, ref: string) {
  return command("git", ["rev-parse", "--verify", "--end-of-options", `${ref}^{commit}`], {
    cwd: root,
    capture: true,
  });
}

/** Read checkout modification state before creating operation outputs or logs. */
export async function workingTreeDirty(root: string) {
  return Boolean(await command("git", ["status", "--porcelain"], { cwd: root, capture: true }));
}

export interface Workspace {
  directory: string;
  tree: string;
  target: string;
}

export interface BuildProvenance {
  rustc: string;
  cargo_encoded_rustflags: string;
  target: string;
  profile: string;
  opt_level: string;
  debug: string;
  features: string[];
  /** Resolved library features from Cargo; absent in historical result files. */
  library_features?: string[];
  rustc_args?: string[];
  revisions?: { libraries: Record<string, string[]> };
  cc?: string;
  cxx?: string;
  native_build?: {
    flags: string[];
    translation_units: { source: string; compiler: string; flags: string[] }[];
  };
}

/** Materialize repository-local links; reject escaping targets and directory cycles.
 *
 * Copies contain regular files and directories, so later source edits cannot
 * change their contents. Both absolute and relative internal links are allowed.
 */
export async function copySnapshot(source: string, destination: string, root: string) {
  const boundary = await realpath(root);
  const ancestors = new Set<string>();
  async function validate(path: string) {
    const target = await realpath(path);
    const name = relative(boundary, target);
    if (name.split(sep)[0] === ".." || isAbsolute(name))
      throw new Error(`Snapshot link escapes repository: ${path}`);
    if (!(await stat(target)).isDirectory()) return;
    if (ancestors.has(target)) throw new Error(`Snapshot directory cycle: ${path}`);
    ancestors.add(target);
    for (const name of await readdir(path)) await validate(join(path, name));
    ancestors.delete(target);
  }
  await validate(source);
  await cp(source, destination, {
    recursive: true,
    dereference: true,
  });
}

/** Materialize a library revision against its own repository, keeping the current harness. */
export async function copyLibraryRevision(root: string, revision: string, destination: string) {
  const directory = await mkdtemp(join(dirname(destination), "baseline-"));
  try {
    const archive = join(directory, "source.tar");
    const tree = join(directory, "tree");
    await mkdir(tree);
    await command("git", ["archive", "--format=tar", `--output=${archive}`, revision], {
      cwd: root,
    });
    await command("tar", ["-xf", archive, "-C", tree]);
    await copySnapshot(join(tree, "rust/tessera-sort"), destination, tree);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

/** Hash relative paths and file contents in lexical order. */
export async function sourceHash(directory: string): Promise<string> {
  const digest = createHash("sha256");
  async function visit(path: string) {
    for (const name of (await readdir(join(directory, path))).sort()) {
      const relative = join(path, name);
      if ((await stat(join(directory, relative))).isDirectory()) await visit(relative);
      else
        digest
          .update(relative)
          .update("\0")
          .update(await readFile(join(directory, relative)));
    }
  }
  await visit("");
  return digest.digest("hex");
}

/** Own an isolated snapshot and clean it up, even on failure.
 *
 * Builds reuse a checkout-local dependency cache. TESSERA_BENCH_TARGET_DIR
 * selects a caller-owned cache (relative to root); never share across trust boundaries.
 */
export async function withWorkspace<T>(
  root: string,
  operation: (workspace: Workspace) => Promise<T>,
): Promise<T> {
  const parent = resolve(
    process.env.TESSERA_BENCH_WORK ?? join(homedir(), "code", "tessera-bench"),
  );
  await mkdir(parent, { recursive: true });
  const directory = await mkdtemp(join(parent, "session-"));
  const tree = join(directory, "tree");
  let added = false;
  try {
    await command("git", ["worktree", "add", "--detach", tree, "HEAD"], { cwd: root });
    added = true;
    for (const path of ["rust/tessera-sort", "bench/rust", "bench/plans"]) {
      await rm(join(tree, path), { recursive: true, force: true });
      await copySnapshot(join(root, path), join(tree, path), root);
    }
    for (const path of ["Cargo.toml", "Cargo.lock", "bench/catalog.toml"]) {
      await copySnapshot(join(root, path), join(tree, path), root);
    }
    for (const path of ["rust-toolchain.toml", "rust-toolchain", ".cargo"]) {
      await rm(join(tree, path), { recursive: true, force: true });
      try {
        await lstat(join(root, path));
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code === "ENOENT") continue;
        throw error;
      }
      await copySnapshot(join(root, path), join(tree, path), root);
    }
    const target = process.env.TESSERA_BENCH_TARGET_DIR
      ? resolve(root, process.env.TESSERA_BENCH_TARGET_DIR)
      : join(root, "target", "tessera-bench");
    return await operation({ directory, tree, target });
  } finally {
    try {
      if (added) await command("git", ["worktree", "remove", "--force", tree], { cwd: root });
    } finally {
      await rm(directory, { recursive: true, force: true });
      await command("git", ["worktree", "prune"], { cwd: root });
    }
  }
}

/** Read Cargo's executable and matching build-script provenance, without guessing paths. */
export async function buildArtifact(output: string, name: string) {
  const messages = output
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line));
  const executables = messages.filter(
    (value) =>
      value.reason === "compiler-artifact" && value.target.name === name && value.executable,
  );
  if (executables.length !== 1) throw new Error(`Expected one ${name} benchmark executable`);
  const artifact = executables[0];
  const script = messages.find(
    (value) => value.reason === "build-script-executed" && value.package_id === artifact.package_id,
  );
  if (!script) throw new Error(`Missing ${name} build provenance`);
  const libraries = messages.filter(
    (value) =>
      value.reason === "compiler-artifact" &&
      value.target.name === "tessera_sort" &&
      value.target.kind?.includes("lib"),
  );
  if (
    libraries.length !== 1 ||
    !Array.isArray(libraries[0].features) ||
    !libraries[0].features.every((feature: unknown) => typeof feature === "string")
  )
    throw new Error("Expected one Tessera library artifact with resolved Cargo features");
  const provenance = JSON.parse(
    await readFile(join(script.out_dir, "provenance.json"), "utf8"),
  ) as BuildProvenance;
  provenance.library_features = [...libraries[0].features].sort();
  for (const [library, features] of Object.entries(provenance.revisions?.libraries ?? {})) {
    const matches = messages.filter(
      (value) =>
        value.reason === "compiler-artifact" &&
        value.target.name === library.replaceAll("-", "_") &&
        value.target.kind?.includes("lib"),
    );
    if (
      matches.length !== 1 ||
      JSON.stringify([...matches[0].features].sort()) !== JSON.stringify(features)
    )
      throw new Error("Revision library features differ from prepared metadata: " + library);
  }
  return { executable: artifact.executable as string, provenance };
}

/** Ask Cargo to resolve flags without compiling the benchmark or its dependencies.
 * A dependency-free build script preserves Cargo's environment/config/target
 * precedence, including encoded arguments containing spaces. The probe is temporary.
 */
export async function benchmarkEnvironment(
  tree: string,
  options: { targetTriple?: string; env?: NodeJS.ProcessEnv } = {},
): Promise<NodeJS.ProcessEnv> {
  const encoded =
    options.env && Object.hasOwn(options.env, "CARGO_ENCODED_RUSTFLAGS")
      ? options.env.CARGO_ENCODED_RUSTFLAGS
      : process.env.CARGO_ENCODED_RUSTFLAGS;
  const defaults = ALIGNMENT_RUSTFLAGS.join("\x1f");
  const environment = (flags: string) => ({
    ...options.env,
    CARGO_ENCODED_RUSTFLAGS: flags.startsWith(defaults)
      ? flags
      : [defaults, flags].filter(Boolean).join("\x1f"),
  });
  if (encoded !== undefined) return environment(encoded);
  const probe = await mkdtemp(join(dirname(tree), "rustflags-"));
  try {
    await writeFile(
      join(probe, "Cargo.toml"),
      `[package]
name = "tessera-flags"
version = "0.0.0"
edition = "2021"
[workspace]
[lib]
path = "lib.rs"
`,
    );
    await writeFile(join(probe, "lib.rs"), "// SPDX-License-Identifier: BSD-2-Clause\n");
    await writeFile(
      join(probe, "build.rs"),
      `// SPDX-License-Identifier: BSD-2-Clause
fn main() {
    std::fs::write(
        std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("flags"),
        std::env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default(),
    ).unwrap();
}
`,
    );
    const output = await command(
      "cargo",
      [
        "check",
        "--offline",
        "--manifest-path",
        join(probe, "Cargo.toml"),
        "--target-dir",
        join(probe, "target"),
        "--message-format=json",
        ...(options.targetTriple ? ["--target", options.targetTriple] : []),
      ],
      { cwd: tree, capture: true, env: options.env },
    );
    const scripts = output
      .split("\n")
      .filter(Boolean)
      .map((line) => JSON.parse(line))
      .filter((message) => message.reason === "build-script-executed");
    if (scripts.length !== 1) throw new Error("Missing Cargo flags probe result");
    const flags = await readFile(join(scripts[0].out_dir, "flags"), "utf8");
    return environment(flags);
  } finally {
    await rm(probe, { recursive: true, force: true });
  }
}

/** Serialize CLI cache writers and finish reading cache-owned files before releasing.
 * Do not nest leases; buildBenchmark returns private workers for use outside it.
 */
export async function withBuildCache<T>(target: string, operation: () => Promise<T>): Promise<T> {
  await mkdir(target, { recursive: true });
  const release = await lockfile.lock(target, {
    lockfilePath: join(target, ".tessera-build-lock"),
    retries: { forever: true, minTimeout: 100, maxTimeout: 1000 },
  });
  try {
    return await operation();
  } finally {
    await release();
  }
}

/** Build and freeze runnable workers with provenance before releasing the shared cache.
 * Returned files belong to the enclosing workspace, never to mutable Cargo output.
 */
export async function buildBenchmark(
  tree: string,
  target: string,
  name: "smoke" | "compare" | "memory",
  features: string[],
  options: {
    targetTriple?: string;
    env?: NodeJS.ProcessEnv;
    rustcArgs?: string[];
    profile?: "dev" | "release" | "bench";
  } = {},
) {
  if (options.rustcArgs && name !== "smoke")
    throw new Error("Final rustc arguments require a smoke benchmark build");
  return withBuildCache(target, async () => {
    const env = await benchmarkEnvironment(tree, options);
    const result = await buildArtifact(
      await command(
        "cargo",
        [
          options.rustcArgs ? "rustc" : "build",
          "--profile",
          options.profile ?? (name === "smoke" ? "bench" : "release"),
          name === "smoke" ? "--bench" : "--bin",
          name,
          ...(name === "compare" ? ["--bin", "memory"] : []),
          "--locked",
          "-p",
          "tessera-bench",
          "--message-format=json",
          "--target-dir",
          target,
          ...(options.targetTriple ? ["--target", options.targetTriple] : []),
          ...(features.length ? ["--features", features.join(",")] : []),
          ...(options.rustcArgs ? ["--", ...options.rustcArgs] : []),
        ],
        { cwd: tree, capture: true, env },
      ),
      name,
    );
    if (options.rustcArgs) result.provenance.rustc_args = options.rustcArgs;
    // Cargo's own lock ends with its process. Keep our lease until provenance and
    // both workers are private, so another experiment cannot substitute its build.
    const frozen = await mkdtemp(join(dirname(tree), "build-"));
    const executable = join(frozen, basename(result.executable));
    await copyFile(result.executable, executable);
    if (name === "compare") {
      const memory = `memory${extname(executable)}`;
      await copyFile(join(dirname(result.executable), memory), join(frozen, memory));
    }
    return { executable, provenance: result.provenance };
  });
}
