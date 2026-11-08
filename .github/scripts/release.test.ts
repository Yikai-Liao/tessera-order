// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { verifyRustRelease } from "./release.ts";

test("release validation enforces Rust version and publication policy", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "tessera-release-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(join(root, "rust/tessera-sort"), { recursive: true });
  const manifest = join(root, "rust/tessera-sort/Cargo.toml");
  await writeFile(manifest, '[package]\nversion = "0.1.0"\n');
  assert.equal((await verifyRustRelease("rust/v0.1.0", root)).state, "verified");
  await assert.rejects(verifyRustRelease("rust/v0.2.0", root), /mismatch/);
  await writeFile(manifest, '[package]\nversion = "0.1.0"\npublish = false\n');
  await assert.rejects(verifyRustRelease("rust/v0.1.0", root), /unpublished/);
});
