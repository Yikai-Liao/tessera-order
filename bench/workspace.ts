// SPDX-License-Identifier: BSD-2-Clause

import { createHash } from "node:crypto";
import { cp, lstat, mkdir, mkdtemp, readdir, readFile, realpath, rm, stat } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { command } from "../tools/process.ts";

/** Align measured Rust code while retaining later, explicitly configured overrides. */
export const ALIGNMENT_RUSTFLAGS = [
  "-C",
  "llvm-args=-align-loops=64",
  "-C",
  "llvm-args=-align-all-nofallthru-blocks=6",
];

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
  rustc_args?: string[];
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

/** Own an isolated source snapshot, one build directory and all cleanup. */
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
    return await operation({ directory, tree, target: join(directory, "target") });
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
  const provenance = JSON.parse(
    await readFile(join(script.out_dir, "provenance.json"), "utf8"),
  ) as BuildProvenance;
  return { executable: artifact.executable as string, provenance };
}

/** Build a runnable benchmark, including the comparison memory worker, with its provenance. */
export async function buildBenchmark(
  tree: string,
  target: string,
  name: "smoke" | "compare",
  features: string[],
  options: { targetTriple?: string; env?: NodeJS.ProcessEnv; rustcArgs?: string[] } = {},
) {
  if (options.rustcArgs && name !== "smoke")
    throw new Error("Final rustc arguments require a smoke benchmark build");
  const build = async (env = options.env) =>
    buildArtifact(
      await command(
        "cargo",
        [
          ...(name === "smoke"
            ? options.rustcArgs
              ? ["rustc", "--profile", "bench", "--bench", name]
              : ["bench", "--bench", name, "--no-run"]
            : ["build", "--release", "--bin", name, "--bin", "memory"]),
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
  const reference = await build();
  const flags = reference.provenance.cargo_encoded_rustflags;
  const aligned = ALIGNMENT_RUSTFLAGS.every((flag) => flags.split("\x1f").includes(flag));
  // Cargo resolves string/array, target and environment settings itself. An
  // encoded override before this build would discard some of those settings.
  const result = aligned
    ? reference
    : await build({
        ...options.env,
        CARGO_ENCODED_RUSTFLAGS: [...ALIGNMENT_RUSTFLAGS, flags].filter(Boolean).join("\x1f"),
      });
  if (options.rustcArgs) result.provenance.rustc_args = options.rustcArgs;
  return result;
}
