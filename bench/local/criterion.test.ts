// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { readEstimates } from "./criterion.ts";

test("criterion results retain IDs and reject duplicates", async () => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-estimates-"));
  const result = async (folder: string, mean: unknown) => {
    const path = join(directory, folder, "new");
    await mkdir(path, { recursive: true });
    await writeFile(
      join(path, "benchmark.json"),
      JSON.stringify({ full_id: "rust/u64/random/65536/s1/api" }),
    );
    await writeFile(
      join(path, "estimates.json"),
      JSON.stringify({ mean: { point_estimate: mean } }),
    );
  };
  try {
    await result("case", 12);
    assert.deepEqual(await readEstimates(directory), { "rust/u64/random/65536/s1/api": 12 });
    await result("case", 0);
    await assert.rejects(readEstimates(directory), /Invalid Criterion/);
    await result("case", 12);
    await result("duplicate", 12);
    await assert.rejects(readEstimates(directory), /Duplicate Criterion/);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
