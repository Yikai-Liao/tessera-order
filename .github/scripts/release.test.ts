// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { collectWheels, verifyRelease } from "./release.ts";

test("release validation matches Rust and Python manifest identities", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "tessera-release-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(join(root, "rust/tessera-sort"), { recursive: true });
  const rust = join(root, "rust/tessera-sort/Cargo.toml");
  await writeFile(rust, '[package]\nversion = "0.1.0"\n');
  assert.equal((await verifyRelease("rust", "rust/v0.1.0", root)).state, "verified");
  await assert.rejects(verifyRelease("rust", "rust/v0.2.0", root), /mismatch/);
  await writeFile(rust, '[package]\nversion = "0.1.0"\npublish = false\n');
  await assert.rejects(verifyRelease("rust", "rust/v0.1.0", root), /unpublished/);
  await mkdir(join(root, "python"));
  await writeFile(join(root, "python/Cargo.toml"), '[package]\nversion = "0.1.0"\n');
  await writeFile(join(root, "python/pyproject.toml"), '[project]\nversion = "0.1.0"\n');
  assert.equal((await verifyRelease("python", "python/v0.1.0", root)).state, "verified");
  await writeFile(join(root, "python/pyproject.toml"), '[project]\nversion = "0.2.0"\n');
  await assert.rejects(verifyRelease("python", "python/v0.1.0", root), /mismatch/);
});

test("wheel collection preserves duplicates and rejects conflicting artifacts", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "tessera-wheels-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  for (const directory of ["a", "b"]) {
    await mkdir(join(root, directory));
    await writeFile(join(root, directory, "package.whl"), "same-build");
  }
  const result = await collectWheels(root, join(root, "out"));
  assert.equal(result.wheels, 1);
  assert.equal(await readFile(join(root, "out/package.whl"), "utf8"), "same-build");
  await writeFile(join(root, "b/package.whl"), "different-build");
  await assert.rejects(collectWheels(root, join(root, "conflict")), /Conflicting wheel/);
  await mkdir(join(root, "empty"));
  await assert.rejects(collectWheels(join(root, "empty"), join(root, "none")), /No wheels/);
});
