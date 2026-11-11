// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { lstat, mkdir, mkdtemp, readdir, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { command } from "../tools/process.ts";
import {
  buildArtifact,
  copyLibraryRevision,
  copySnapshot,
  sourceHash,
  withWorkspace,
} from "./workspace.ts";

/** Commit fixture inputs without depending on the caller's identity or hooks. */
async function commit(root: string, message: string) {
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
      message,
    ],
    { cwd: root },
  );
}

test("snapshots preserve dirty sources and clean up failures", async () => {
  const root = await mkdtemp(join(tmpdir(), "tessera-workspace-test-"));
  try {
    for (const path of ["rust/tessera-sort/src", "bench/rust/src", "bench/plans"])
      await mkdir(join(root, path), { recursive: true });
    for (const path of [
      "Cargo.toml",
      "Cargo.lock",
      "rust-toolchain.toml",
      "bench/catalog.toml",
      "bench/plans/continuous.toml",
      "rust/tessera-sort/src/lib.rs",
      "bench/rust/src/lib.rs",
    ])
      await writeFile(join(root, path), "original\n");
    await command("git", ["init", "-q", root], { capture: true });
    await commit(root, "fixture");
    await writeFile(join(root, "rust/tessera-sort/src/lib.rs"), "modified\n");
    await writeFile(join(root, "rust/tessera-sort/src/new.rs"), "untracked\n");
    await writeFile(join(root, "bench/plans/continuous.toml"), "dirty plan\n");
    const crate = join(root, "rust/tessera-sort");
    await symlink("new.rs", join(crate, "src/relative.rs"));
    await symlink(join(crate, "src/new.rs"), join(crate, "src/absolute.rs"));
    await symlink("src", join(crate, "linked-directory"), "dir");
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
        const snapshot = join(workspace.tree, "rust/tessera-sort");
        const hash = await sourceHash(snapshot);
        await writeFile(join(crate, "src/new.rs"), "changed after snapshot\n");
        for (const name of ["src/relative.rs", "src/absolute.rs", "linked-directory/new.rs"]) {
          assert.equal(await readFile(join(snapshot, name), "utf8"), "untracked\n");
          assert.equal((await lstat(join(snapshot, name))).isSymbolicLink(), false);
        }
        assert.equal(await sourceHash(snapshot), hash);
        throw new Error("operation failed");
      }),
      /operation failed/,
    );
    await assert.rejects(readFile(join(directory, "tree/.git")), { code: "ENOENT" });
    assert.equal(
      await command("git", ["status", "--porcelain"], { cwd: root, capture: true }),
      before,
    );
    const cached = join(root, "../", `target-${process.pid}`);
    const previous = process.env.TESSERA_BENCH_TARGET_DIR;
    try {
      process.env.TESSERA_BENCH_TARGET_DIR = cached;
      await withWorkspace(root, async (workspace) => {
        assert.equal(workspace.target, cached);
        await mkdir(cached, { recursive: true });
        await writeFile(join(cached, "dependency"), "cached");
      });
      assert.equal(await readFile(join(cached, "dependency"), "utf8"), "cached");
    } finally {
      if (previous === undefined) delete process.env.TESSERA_BENCH_TARGET_DIR;
      else process.env.TESSERA_BENCH_TARGET_DIR = previous;
      await rm(cached, { recursive: true, force: true });
    }
    const escaping = join(crate, "src/escaping.rs");
    await symlink(process.execPath, escaping);
    await assert.rejects(
      withWorkspace(root, async () => assert.fail("Escaping input reached the build")),
      /Snapshot link escapes repository/,
    );
    await rm(escaping);
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

test("baseline links resolve in their own revision without replacing the harness", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-revision-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const root = join(directory, "repository");
  await mkdir(join(root, "rust/tessera-sort/src"), { recursive: true });
  await writeFile(join(root, "shared.rs"), "old implementation");
  await writeFile(join(root, "Cargo.lock"), "old lockfile");
  await mkdir(join(root, "bench/rust"), { recursive: true });
  await writeFile(join(root, "bench/rust/harness.rs"), "old harness");
  await symlink("../../../shared.rs", join(root, "rust/tessera-sort/src/shared.rs"));
  await command("git", ["init", "-q", root], { capture: true });
  await commit(root, "baseline");
  await writeFile(join(root, "shared.rs"), "new implementation");
  await writeFile(join(root, "Cargo.lock"), "current lockfile");
  await writeFile(join(root, "bench/rust/harness.rs"), "current harness");
  await commit(root, "current");
  const tree = join(directory, "current-tree");
  await copySnapshot(root, tree, root);
  const crate = join(tree, "rust/tessera-sort");
  assert.equal(await readFile(join(crate, "src/shared.rs"), "utf8"), "new implementation");
  await rm(crate, { recursive: true });
  await copyLibraryRevision(root, "HEAD~1", crate);
  assert.equal(await readFile(join(crate, "src/shared.rs"), "utf8"), "old implementation");
  assert.equal((await lstat(join(crate, "src/shared.rs"))).isSymbolicLink(), false);
  assert.equal(await readFile(join(tree, "Cargo.lock"), "utf8"), "current lockfile");
  assert.equal(await readFile(join(tree, "bench/rust/harness.rs"), "utf8"), "current harness");
  assert.deepEqual(await readdir(join(tree, "rust")), ["tessera-sort"]);
  await assert.rejects(copyLibraryRevision(root, "missing-revision", join(tree, "rust/missing")));
  assert.deepEqual(await readdir(join(tree, "rust")), ["tessera-sort"]);
});

test("snapshot boundaries reject escapes and cycles", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-links-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const root = join(directory, "source");
  await mkdir(root);
  await writeFile(join(directory, "outside"), "outside");
  const link = join(root, "link");
  for (const target of ["../outside", join(directory, "outside"), "."]) {
    await symlink(target, link);
    await assert.rejects(
      copySnapshot(root, join(directory, "copy"), root),
      target === "." ? /directory cycle/ : /escapes repository/,
    );
    await rm(link);
  }
});

test("Cargo artifacts retain paths and build provenance", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-artifact-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const provenance = {
    rustc: "configured compiler",
    cargo_encoded_rustflags: "--cfg\u001fconfigured",
    target: "configured-target",
    profile: "release",
    opt_level: "3",
    debug: "false",
    features: [],
  };
  await writeFile(join(directory, "provenance.json"), JSON.stringify(provenance));
  for (const [name, executable] of [
    ["compare", "/target/configured-target/release/compare"],
    ["smoke", "C:\\target\\configured-target\\release\\deps\\smoke.exe"],
  ]) {
    for (const library_features of [[], ["default", "portable-simd", "std"]]) {
      const messages = [
        { reason: "compiler-artifact", target: { name: "dependency" }, executable: "/unrelated" },
        { reason: "build-script-executed", package_id: "measured", out_dir: directory },
        { reason: "compiler-artifact", package_id: "measured", target: { name }, executable },
        {
          reason: "compiler-artifact",
          package_id: "library",
          target: { name: "tessera_sort", kind: ["lib"] },
          features: library_features,
        },
      ];
      const output = messages.map((message) => JSON.stringify(message)).join("\n");
      assert.deepEqual(await buildArtifact(output, name), {
        executable,
        provenance: { ...provenance, library_features },
      });
      const revision = {
        revisions: {
          libraries: { "tessera-sort": library_features, "revision-baseline": ["portable-simd"] },
        },
      };
      await writeFile(
        join(directory, "provenance.json"),
        JSON.stringify({ ...provenance, ...revision }),
      );
      await assert.rejects(buildArtifact(output, name), /features differ/);
      const clone = {
        reason: "compiler-artifact",
        target: { name: "revision_baseline", kind: ["lib"] },
        features: ["portable-simd"],
      };
      const linked = output + "\n" + JSON.stringify(clone);
      assert.deepEqual(
        (await buildArtifact(linked, name)).provenance.revisions,
        revision.revisions,
      );
      clone.features = [];
      await assert.rejects(
        buildArtifact(output + "\n" + JSON.stringify(clone), name),
        /features differ/,
      );
      await writeFile(join(directory, "provenance.json"), JSON.stringify(provenance));
      await assert.rejects(buildArtifact("", name), /Expected one/);
      await assert.rejects(
        buildArtifact(JSON.stringify(messages[2]), name),
        /Missing .* provenance/,
      );
      for (const artifact of [messages[2], messages[3]])
        await assert.rejects(
          buildArtifact(`${output}\n${JSON.stringify(artifact)}`, name),
          /Expected one/,
        );
      await assert.rejects(
        buildArtifact(
          messages
            .slice(0, 3)
            .map((message) => JSON.stringify(message))
            .join("\n"),
          name,
        ),
        /Expected one Tessera library artifact/,
      );
    }
  }
});
