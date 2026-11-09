// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import type { LocalBlock } from "./model.ts";
import { type ProtocolRun, protocolSeeds, summarizeProtocols } from "./protocol.ts";
import { renderLocalReport } from "./report.ts";

const id = "rust/u32/random/48/s1/api";
const controlId = "rust/u64/random/65536/s1/api";

function fixture(): ProtocolRun {
  const timing = { warm_up_ms: 500, measurement_ms: 1500, samples: 30 };
  const divided = { warm_up_ms: 84, measurement_ms: 250, samples: 10 };
  const build = {
    rustc: "rustc",
    cargo_encoded_rustflags: "",
    target: "x86_64-unknown-linux-gnu",
    profile: "release",
    opt_level: "3",
    debug: "true",
    features: ["CARGO_FEATURE_LAYOUT_STUDY"],
  };
  const block = (duration: number): LocalBlock => ({
    baseline: { [id]: 100, [controlId]: 100 },
    control: { [id]: 100, [controlId]: 100 },
    current: { [id]: 100, [controlId]: 100 },
    elapsedMs: { baseline: duration, control: duration * 100, current: duration },
  });
  return {
    schema: 3,
    protocol: "candidate-only-vs-paired-median3-v1",
    expected: "same",
    controls: [],
    timings: {
      single: { [id]: timing, [controlId]: timing },
      multiple: { [id]: divided, [controlId]: divided },
    },
    linker: { driver: "cc", version: "LLD" },
    provenance: {
      baseline: "commit",
      head: "commit",
      dirty: false,
      source: { baseline: "same", current: "same", harness: "harness", catalog: "catalog" },
      rustc: "rustc",
      flags: "",
      configuration: "default",
      host: { platform: "linux", release: "test", arch: "x64", cpu: "test", affinity: "2" },
      cases: [id, controlId],
    },
    trials: Array.from({ length: 9 }, (_, trial) => ({
      pairs: Array.from({ length: 3 }, (_, pair) => ({
        baselineSeed: 101 + trial * 6 + pair * 2,
        currentSeed: 102 + trial * 6 + pair * 2,
        binaries: { baseline: `baseline-${trial}-${pair}`, current: `current-${trial}-${pair}` },
        builds: { baseline: build, current: build },
      })),
      singlePair: trial % 3,
      single: block(10),
      multiple: Array.from({ length: 3 }, () => block(1)),
      wallMs: { single: 1025, multiple: 312 },
    })),
  };
}

test("protocol cohorts reserve distinct bounded held-out seeds", () => {
  const seeds = protocolSeeds(101);
  assert.equal(seeds.length, 54);
  assert.equal(new Set(seeds).size, 54);
  assert.equal(seeds.at(-1), 154);
  assert.equal(protocolSeeds(4294967242).at(-1), 4294967295);
  for (const first of [0, -1, 4294967243, NaN])
    assert.throws(() => protocolSeeds(first), /layout seeds/);
});

test("budget charges candidate-only versus three baseline/current pairs", () => {
  const run = fixture();
  for (const trial of run.trials) {
    trial.single!.control[id] = 1000;
    trial.multiple[0].control[id] = 1000;
  }
  const summary = summarizeProtocols(run);
  assert.equal(summary.jobCost.ratio, 0.6);
  assert.equal(summary.jobCost.trials[0].singleMs, 10);
  assert.equal(summary.jobCost.trials[0].multipleMs, 6);
  assert.equal(summary.jobCost.fitsObservedBudget, true);
  assert.equal(summary.wallTime.ratio, 312 / 1025);
  assert.equal(summary.cases[0].single.ratio, 1);
  assert.equal(summary.cases[0].multiple.ratio, 1);
  assert.equal(summary.cases[0].single.withinLimits, 9);
  assert.ok(summary.cases[0].single.aaHigh > 1.8);
});

test("median-of-three can reject a placement outlier without changing the fixed limits", () => {
  const run = fixture();
  for (const trial of run.trials) {
    trial.single!.current[id] = 90;
    trial.multiple[0].current[id] = 90;
  }
  const summary = summarizeProtocols(run).cases[0];
  assert.equal(summary.single.faster, 9);
  assert.equal(summary.single.matchesExpectation, 0);
  assert.equal(summary.multiple.withinLimits, 9);
  assert.equal(summary.multiple.matchesExpectation, 9);
});

test("a bimodal layout cohort can make median-of-three less reliable", () => {
  const run = fixture();
  for (const trial of run.trials) {
    trial.multiple[2].baseline[id] = 200;
    trial.multiple[1].current[id] = 200;
    trial.multiple[2].current[id] = 200;
  }
  const summary = summarizeProtocols(run).cases[0];
  assert.equal(summary.single.withinLimits, 9);
  assert.equal(summary.multiple.slower, 9);
});

test("historical direction and unaffected controls remain separate hypotheses", () => {
  const run = fixture();
  run.expected = "faster";
  run.provenance.source.current = "changed";
  run.controls = [controlId];
  for (const trial of run.trials)
    for (const block of [trial.single!, ...trial.multiple]) block.current[id] = 90;
  const summary = summarizeProtocols(run);
  assert.equal(summary.cases[0].expected, "faster");
  assert.equal(summary.cases[0].multiple.matchesExpectation, 9);
  assert.equal(summary.cases[1].expected, "same");
  assert.equal(summary.cases[1].multiple.matchesExpectation, 9);
});

test("protocol reports reject incomplete, mismatched and under-accounted results", () => {
  const edits: [(run: ProtocolRun) => void, RegExp][] = [
    [
      (run) => {
        run.trials.pop();
      },
      /nine protocol trials/,
    ],
    [
      (run) => {
        run.trials[0].single = undefined;
      },
      /nine complete/,
    ],
    [
      (run) => {
        run.trials[0].multiple[1] = null as unknown as LocalBlock;
      },
      /nine complete/,
    ],
    [
      (run) => {
        run.trials[0].single!.elapsedMs = undefined;
      },
      /elapsed times/,
    ],
    [
      (run) => {
        run.trials[0].wallMs.multiple = undefined;
      },
      /strategy wall times/,
    ],
    [
      (run) => {
        run.trials[0].wallMs.multiple = 1;
      },
      /smaller than its recorded processes/,
    ],
    [
      (run) => {
        run.trials[0].pairs[0].currentSeed = 101;
      },
      /distinct integer/,
    ],
    [
      (run) => {
        run.trials[0].pairs[1].binaries.baseline = "baseline-0-0";
      },
      /distinct executables/,
    ],
    [
      (run) => {
        run.provenance.source.current = "different";
      },
      /Source hashes/,
    ],
    [
      (run) => {
        run.timings!.multiple[id].samples = 5;
      },
      /timing settings/,
    ],
    [
      (run) => {
        run.timings!.single[id].measurement_ms = Infinity;
      },
      /timing settings/,
    ],
    [
      (run) => {
        run.trials[0].single!.current = {};
      },
      /selected cases/,
    ],
  ];
  for (const [edit, message] of edits) {
    const run = structuredClone(fixture());
    edit(run);
    assert.throws(() => summarizeProtocols(run), message);
  }
});

test("retained protocol results regenerate without measuring or rebuilding", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-protocol-report-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  await writeFile(join(directory, "local.json"), JSON.stringify(fixture()));
  await renderLocalReport(directory);
  assert.deepEqual(
    JSON.parse(await readFile(join(directory, "summaries.json"), "utf8")),
    summarizeProtocols(fixture()),
  );
  const report = await readFile(join(directory, "index.md"), "utf8");
  assert.match(report, /at most \*\*1\.0000/);
  assert.match(report, /Diagnostic control processes never enter either score/);
  assert.match(report, /not a population false-positive rate/);
});
