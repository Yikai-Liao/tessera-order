// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { command } from "./process.ts";
import { withWorkspace } from "./workspace.ts";

test("snapshots preserve dirty sources and clean up failures", async () => {
  const root = await mkdtemp(join(tmpdir(), "tessera-workspace-test-"));
  try {
    for (const path of ["rust/tessera-sort/src", "bench/rust/src", "bench/plans"])
      await mkdir(join(root, path), { recursive: true });
    for (const path of [
      "Cargo.toml",
      "Cargo.lock",
      "rust-toolchain.toml",
      "bench/catalog.json",
      "bench/plans/continuous.toml",
      "rust/tessera-sort/src/lib.rs",
      "bench/rust/src/lib.rs",
    ])
      await writeFile(join(root, path), "original\n");
    await command("git", ["init", "-q", root], { capture: true });
    await command("git", ["add", "."], { cwd: root });
    await command(
      "git",
      [
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.org",
        "-c",
        "commit.gpgsign=false",
        "-c",
        "core.hooksPath=/dev/null",
        "commit",
        "-qm",
        "fixture",
      ],
      { cwd: root },
    );
    await writeFile(join(root, "rust/tessera-sort/src/lib.rs"), "modified\n");
    await writeFile(join(root, "rust/tessera-sort/src/new.rs"), "untracked\n");
    await writeFile(join(root, "bench/plans/continuous.toml"), "dirty plan\n");
    const before = await command("git", ["status", "--porcelain"], { cwd: root, capture: true });
    let directory = "";
    await assert.rejects(
      withWorkspace(root, async (workspace) => {
        directory = workspace.directory;
        assert.equal(
          await readFile(join(workspace.tree, "rust/tessera-sort/src/lib.rs"), "utf8"),
          "modified\n",
        );
        assert.equal(
          await readFile(join(workspace.tree, "rust/tessera-sort/src/new.rs"), "utf8"),
          "untracked\n",
        );
        assert.equal(
          await readFile(join(workspace.tree, "bench/plans/continuous.toml"), "utf8"),
          "dirty plan\n",
        );
        throw new Error("operation failed");
      }),
      /operation failed/,
    );
    await assert.rejects(readFile(join(directory, "tree/.git")), { code: "ENOENT" });
    assert.equal(
      await command("git", ["status", "--porcelain"], { cwd: root, capture: true }),
      before,
    );
    assert.equal(
      (
        await command("git", ["worktree", "list", "--porcelain"], { cwd: root, capture: true })
      ).split("worktree ").length,
      2,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
