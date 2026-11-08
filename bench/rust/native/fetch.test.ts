// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { c as archive } from "tar";
import { fetchNativeSource } from "./fetch.ts";
import { verifySources } from "./sources.ts";

test("fetch staging preserves sources until verification and installs beside the destination", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "tessera-source-install-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const vendor = join(root, "vendor");
  const target = join(vendor, "sample");
  await mkdir(target, { recursive: true });
  await writeFile(join(target, "old"), "previous source");
  await mkdir(join(root, "source"));
  await writeFile(join(root, "source/LICENSE"), "BSD-2-Clause");
  const path = join(root, "source.tar.gz");
  await archive({ cwd: root, file: path, gzip: true }, ["source"]);
  const bytes = new Uint8Array(await readFile(path));
  const source = {
    repository: "https://github.com/owner/source",
    commit: "a".repeat(40),
    sha256: createHash("sha256").update(bytes).digest("hex"),
    license: "BSD-2-Clause",
    license_files: ["LICENSE"],
  };
  let valid = false;
  let calls = 0;
  t.mock.method(globalThis, "fetch", async () => {
    calls++;
    assert.ok((await readdir(vendor)).some((name) => name.startsWith(".source-")));
    return new Response(valid ? bytes : new Uint8Array([0]));
  });
  await assert.rejects(fetchNativeSource(vendor, "sample", source), /SHA-256 mismatch/);
  assert.equal(await readFile(join(target, "old"), "utf8"), "previous source");
  assert.deepEqual(await readdir(vendor), ["sample"]);
  valid = true;
  assert.equal(await fetchNativeSource(vendor, "sample", source), "fetched");
  assert.deepEqual(await readdir(vendor), ["sample"]);
  assert.equal(await readFile(join(target, "LICENSE"), "utf8"), "BSD-2-Clause");
  await verifySources(vendor, { sample: source });
  assert.equal(await fetchNativeSource(vendor, "sample", source), "verified");
  assert.equal(calls, 2);
});
