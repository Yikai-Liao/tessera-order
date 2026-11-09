// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import {
  engineOrderingSymbols,
  type LayoutRun,
  parseLayoutSeeds,
  summarizeLayouts,
} from "./layout.ts";
import type { LocalRun } from "./model.ts";
import { renderLocalReport } from "./report.ts";

const id = "rust/u32/random/48/s1/api";

test("engine ordering uses function ownership, excluding harness generic mentions", () => {
  const raw =
    "engine t 10 8\nkey t 20 8\nharness t 30 8\npartition t 40 8\nnested t 50 8\nvalue r 60 8";
  const names = [
    "tessera_sort::engine::Sorter::<u64>::sort t 10 8",
    "<u64 as tessera_sort::key::sealed::Sealed>::insertion_sort t 20 8",
    "criterion::Bencher::iter::<tessera_sort::stable_sort<u64>> t 30 8",
    "<tessera_sort::block::Partition<u64>>::new t 40 8",
    "<harness::Wrapper<<u64 as tessera_sort::Key>::Repr> as core::Drop>::drop t 50 8",
    "harness::value r 60 8",
  ].join("\n");
  assert.deepEqual(engineOrderingSymbols(raw, names), ["engine", "key", "partition"]);
  assert.throws(
    () => engineOrderingSymbols(raw, names.replace("t 10 8", "t 11 8")),
    /views differ/,
  );
  assert.throws(
    () => engineOrderingSymbols(raw, names.split("\n").slice(1).join("\n")),
    /views differ/,
  );
});

/** Model identical source with layout-dependent time and separate process noise. */
function fixture(): LayoutRun {
  return {
    schema: 2,
    linker: { driver: "cc", version: "LLD" },
    layouts: [100, 120, 90].map((time, index) => ({
      seed: index + 1,
      run: {
        schema: 1,
        baseline: "commit",
        head: "commit",
        dirty: false,
        source: { baseline: "same", current: "same", harness: "harness", catalog: "catalog" },
        binaries: { baseline: `base-${index}`, current: `current-${index}` },
        rustc: "rustc",
        flags: "",
        configuration: "default",
        host: { platform: "linux", release: "test", arch: "x64", cpu: "test", affinity: "2" },
        cases: [id],
        blocks: Array.from({ length: 9 }, () => ({
          baseline: { [id]: time },
          control: { [id]: time * 1.02 },
          current: { [id]: time * (index === 0 ? 1.2 : 1) },
        })),
      } satisfies LocalRun,
    })),
  };
}

test("layout seeds are fixed distinct unsigned values", () => {
  assert.deepEqual(parseLayoutSeeds("1, 42,4294967295"), [1, 42, 4294967295]);
  for (const value of ["", "0", "-1", "1,1", "1,01", "1,", "1.2", "1e2", "4294967296"])
    assert.throws(() => parseLayoutSeeds(value), /distinct integer layout seeds/);
});

test("layout dispersion stays separate from the process A/A screen", () => {
  const [summary] = summarizeLayouts(fixture());
  assert.equal(summary.ratio, 1 / 1.01);
  assert.equal(summary.low, 1 / 1.01);
  assert.equal(summary.high, 1.2 / 1.01);
  assert.ok(Math.abs(summary.baselineSpread - (120 / 90 - 1)) < 1e-12);
  assert.ok(Math.abs(summary.currentSpread - (120 / 90 - 1)) < 1e-12);
  assert.equal(summary.layouts[0].verdict, "slower");
  assert.equal(summary.layouts[0].baselineLowNs, 101);
  assert.equal(summary.layouts[0].currentHighNs, 120);
  assert.equal(summary.layouts[1].verdict, "within noise");
  assert.ok(!("verdict" in summary));
});

test("layout reports reject incomplete, mismatched and ineffective runs", () => {
  const incomplete = fixture();
  incomplete.layouts[2].run.blocks.pop();
  assert.throws(() => summarizeLayouts(incomplete), /nine complete/);
  const mismatched = fixture();
  mismatched.layouts[1].run.source.current = "different";
  assert.throws(() => summarizeLayouts(mismatched), /sources/);
  const ineffective = fixture();
  ineffective.layouts[1].run.binaries.baseline = ineffective.layouts[0].run.binaries.baseline;
  assert.throws(() => summarizeLayouts(ineffective), /identical baseline executables/);
  const duplicate = fixture();
  duplicate.layouts[1].seed = duplicate.layouts[0].seed;
  assert.throws(() => summarizeLayouts(duplicate), /distinct integer/);
});

test("retained multi-layout results regenerate without measurement", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-layout-report-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  await writeFile(join(directory, "local.json"), JSON.stringify(fixture()));
  await renderLocalReport(directory);
  const summary = JSON.parse(await readFile(join(directory, "summaries.json"), "utf8"));
  assert.deepEqual(summary, summarizeLayouts(fixture()));
  const markdown = await readFile(join(directory, "index.md"), "utf8");
  assert.match(markdown, /no pooled regression verdict/);
  assert.match(markdown, /Layout seed 3/);
  assert.match(await readFile(join(directory, "local.svg"), "utf8"), /LAYOUT SEED 2/);
  const absolute = await readFile(join(directory, "layout-1.svg"), "utf8");
  assert.match(absolute, /Layout times · u32 \/ random \/ 48/);
  assert.match(absolute, /Seed 3 · baseline/);
  assert.match(absolute, /not confidence intervals/);
});

test("original single-layout reports remain readable", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-local-report-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  await writeFile(join(directory, "local.json"), JSON.stringify(fixture().layouts[0].run));
  await renderLocalReport(directory);
  const [summary] = JSON.parse(await readFile(join(directory, "summaries.json"), "utf8"));
  assert.equal(summary.verdict, "slower");
  assert.equal(summary.ratio, 1.2 / 1.01);
  assert.ok(!("layouts" in summary));
});
