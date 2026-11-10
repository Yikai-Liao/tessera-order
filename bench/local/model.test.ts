// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { summarizeLayouts } from "./layout.ts";
import { type LocalRun, summarizeLocal } from "./model.ts";
import { renderLocalReport } from "./report.ts";

test("local budgets distinguish short screens from interrupted historical runs", async (t) => {
  const run: LocalRun = {
    schema: 1,
    baseline: "same",
    head: "same",
    dirty: false,
    source: { baseline: "same", current: "same", harness: "same", catalog: "same" },
    binaries: { baseline: "first", current: "second" },
    rustc: "fixture",
    flags: "",
    configuration: "portable-simd",
    host: { platform: "linux", release: "fixture", arch: "x64", cpu: "fixture", affinity: "0" },
    cases: ["rust/u64/random/1000/s1/api"],
    blockCount: 3,
    blocks: Array.from({ length: 3 }, () => ({
      baseline: { "rust/u64/random/1000/s1/api": 100 },
      control: { "rust/u64/random/1000/s1/api": 120 },
      current: { "rust/u64/random/1000/s1/api": 109 },
    })),
  };
  assert.equal(summarizeLocal(run)[0].verdict, "inconclusive");
  assert.throws(() => summarizeLocal({ ...run, blockCount: undefined }), /9 complete blocks/);
  assert.throws(() => summarizeLocal({ ...run, blocks: run.blocks.slice(1) }), /3 complete blocks/);
  assert.throws(() => summarizeLocal({ ...run, blockCount: 2 }), /budget/);
  assert.throws(
    () =>
      summarizeLayouts({
        schema: 2,
        linker: { driver: "fixture", version: "fixture" },
        layouts: [
          { seed: 1, run },
          { seed: 2, run: { ...run, blockCount: 9 } },
        ],
      }),
    /block budget/,
  );
  const out = await mkdtemp(join(tmpdir(), "tessera-short-local-"));
  t.after(() => rm(out, { recursive: true, force: true }));
  await renderLocalReport(out, run);
  assert.match(await readFile(join(out, "index.md"), "utf8"), /3 blocks per case \(short screen\)/);
  assert.match(await readFile(join(out, "local.svg"), "utf8"), /3 process-level blocks/);
});
