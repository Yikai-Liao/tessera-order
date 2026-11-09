// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import { parse } from "smol-toml";
import { serialHarnessManifest } from "./runner.ts";

test("serial source builds retain one harness without requiring the parallel library API", async () => {
  const original = await readFile(new URL("../rust/Cargo.toml", import.meta.url), "utf8");
  const before = parse(original);
  const normalized = serialHarnessManifest(original);
  const after = parse(normalized);
  const features = before.features as Record<string, string[]>;
  assert.ok(features.rayon.includes("tessera-sort/rayon"));
  assert.ok(features.compare.includes("dep:rayon"));
  features.rayon = features.rayon.filter((feature) => feature !== "tessera-sort/rayon");
  assert.deepEqual(after, before);
  assert.equal(serialHarnessManifest(normalized), normalized);
});
