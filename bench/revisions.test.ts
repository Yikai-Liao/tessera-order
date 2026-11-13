// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { parse } from "smol-toml";
import { command } from "../tools/process.ts";
import { prepareRevisions, verifyRevisionLock } from "./revisions.ts";
import { sourceHash, withWorkspace } from "./workspace.ts";

test("revision preparation preserves external lock entries", () => {
  const original =
    '[[package]]\nname="dependency"\nversion="1.0.0"\nsource="registry+test"\nchecksum="abc"\n';
  verifyRevisionLock(
    original,
    `${original}[[package]]\nname="revision-baseline"\nversion="0.0.0"\n`,
  );
  for (const changed of [original.replace("1.0.0", "2.0.0"), original.replace("abc", "def"), ""]) {
    assert.throws(() => verifyRevisionLock(original, changed), /differ from the current lockfile/);
  }
});

/** Minimal offline Cargo workspace with no registry dependencies. */
async function fixture() {
  const root = await mkdtemp(join(tmpdir(), "tessera-revisions-test-"));
  for (const path of ["rust/tessera-order/src", "bench/rust/src", "bench/plans"])
    await mkdir(join(root, path), { recursive: true });
  const files = {
    "Cargo.toml": '[workspace]\nmembers=["rust/tessera-order","bench/rust"]\nresolver="3"\n',
    "rust/tessera-order/Cargo.toml":
      '[package]\nname="tessera-order"\nversion="0.0.0"\n[features]\nportable-simd=[]\n',
    "bench/rust/Cargo.toml":
      '[package]\nname="tessera-bench"\nversion="0.0.0"\n[dependencies]\ntessera-order={path="../../rust/tessera-order"}\n[features]\ncompare=[]\n',
    "rust/tessera-order/src/lib.rs": "// original library\n",
    "bench/rust/src/lib.rs": "// current harness\n",
    "bench/catalog.toml": "schema=1\n",
  };
  for (const [path, content] of Object.entries(files)) await writeFile(join(root, path), content);
  await command("cargo", ["generate-lockfile", "--offline"], { cwd: root });
  await command("git", ["init", "-q", root]);
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
  return root;
}

for (const explicit of [false, true]) {
  test(`revision copies retain source identity: ${explicit ? "commit" : "working tree"}`, async (t) => {
    const root = await fixture();
    t.after(() => rm(root, { recursive: true, force: true }));
    await writeFile(join(root, "rust/tessera-order/src/lib.rs"), "// dirty library\n");
    await writeFile(join(root, "bench/rust/src/lib.rs"), "// dirty harness\n");
    const originalLock = parse(await readFile(join(root, "Cargo.lock"), "utf8"));
    await withWorkspace(root, async ({ tree }) => {
      await prepareRevisions(root, tree, {
        baseline: "HEAD",
        current: explicit ? "HEAD" : undefined,
      });
      const revision = JSON.parse(await readFile(join(tree, "bench/rust/revisions.json"), "utf8"));
      assert.equal(revision.workingTree, !explicit);
      for (const features of Object.values(revision.libraries))
        assert.deepEqual(features, ["portable-simd"]);
      assert.equal(revision.currentSource, revision.copies["revision-current-copy"]);
      assert.equal(revision.copies["revision-baseline"], revision.copies["revision-baseline-copy"]);
      assert.equal(revision.currentSource === revision.copies["revision-baseline"], explicit);
      assert.equal(revision.currentSource, await sourceHash(join(tree, "rust/tessera-order")));
      for (const name of Object.keys(revision.copies)) {
        const path = join(tree, "bench/revision-libraries", name);
        const manifest = parse(await readFile(join(path, "Cargo.toml"), "utf8"));
        assert.equal((manifest.package as { name: string }).name, name);
        assert.equal(
          await readFile(join(path, "src/lib.rs"), "utf8"),
          name === "revision-current-copy" && !explicit
            ? "// dirty library\n"
            : "// original library\n",
        );
      }
      assert.equal(
        await readFile(join(tree, "bench/rust/src/lib.rs"), "utf8"),
        "// dirty harness\n",
      );
      const lock = parse(await readFile(join(tree, "Cargo.lock"), "utf8"));
      const packages = lock.package as { name: string }[];
      assert.deepEqual(
        packages.filter((p) => !p.name.startsWith("revision-")).map((p) => ({ ...p })),
        (originalLock.package as { name: string }[]).map((p) =>
          p.name === "tessera-bench"
            ? {
                ...p,
                dependencies: [
                  "revision-baseline",
                  "revision-baseline-copy",
                  "revision-current-copy",
                  "tessera-order",
                ],
              }
            : { ...p },
        ),
      );
    });
    assert.equal(
      await readFile(join(root, "rust/tessera-order/src/lib.rs"), "utf8"),
      "// dirty library\n",
    );
  });
}
