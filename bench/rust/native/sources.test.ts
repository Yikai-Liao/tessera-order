// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { nativeSources, treeHash, verifySources } from "./sources.ts";

test("source hashes preserve existing stamps and reject changes or escaping links", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "tessera-native-source-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const tree = join(root, "sample");
  await mkdir(join(tree, "dir"), { recursive: true });
  await writeFile(join(tree, "dir/b"), "second");
  await writeFile(join(tree, "a"), "first");
  const hash = createHash("sha256");
  for (const [name, content] of [
    ["a", "first"],
    ["dir/b", "second"],
  ])
    hash.update(name).update("\0").update(createHash("sha256").update(content).digest());
  assert.equal(await treeHash(tree), hash.digest("hex"));
  const source = {
    repository: "https://github.com/owner/source",
    commit: "a".repeat(40),
    sha256: "b".repeat(64),
    license: "BSD-2-Clause",
    license_files: ["a"],
  };
  await writeFile(
    join(tree, ".tessera-source.json"),
    JSON.stringify({ ...source, tree_sha256: await treeHash(tree) }),
  );
  await verifySources(root, { sample: source });
  await writeFile(join(tree, "a"), "changed");
  await assert.rejects(verifySources(root, { sample: source }), /source tree differs/);
  await writeFile(join(root, "outside"), "outside");
  await symlink("../outside", join(tree, "escape"));
  await assert.rejects(treeHash(tree), /escapes/);
  assert.equal(Object.keys(await nativeSources()).length, 7);
});
