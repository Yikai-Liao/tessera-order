// SPDX-License-Identifier: BSD-2-Clause

import { createHash } from "node:crypto";
import { cp, mkdir, mkdtemp, readdir, readFile, rm, stat } from "node:fs/promises";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { command } from "./process.ts";

export interface Workspace {
  directory: string;
  tree: string;
  target: string;
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
      await cp(join(root, path), join(tree, path), {
        recursive: true,
        filter: (source) => !source.split("/").includes("__pycache__"),
      });
    }
    for (const path of ["Cargo.toml", "Cargo.lock", "bench/catalog.json"]) {
      await cp(join(root, path), join(tree, path));
    }
    for (const path of ["rust-toolchain.toml", "rust-toolchain", ".cargo"]) {
      await rm(join(tree, path), { recursive: true, force: true });
      try {
        await cp(join(root, path), join(tree, path), { recursive: true });
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
      }
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

/** Build the continuous harness and return its single executable. */
export async function buildSmoke(tree: string, target: string, simd: boolean): Promise<string> {
  const output = await command(
    "cargo",
    [
      "bench",
      "--locked",
      "-p",
      "tessera-bench",
      "--bench",
      "smoke",
      "--no-run",
      "--message-format=json",
      "--target-dir",
      target,
      ...(simd ? ["--features", "portable-simd"] : []),
    ],
    { cwd: tree, capture: true },
  );
  const executables = output.split("\n").flatMap((line) => {
    const value = JSON.parse(line);
    return value.reason === "compiler-artifact" && value.target.name === "smoke" && value.executable
      ? [value.executable as string]
      : [];
  });
  if (executables.length !== 1) throw new Error("Expected one smoke benchmark executable");
  return executables[0];
}
