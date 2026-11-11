// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type TestContext, test } from "node:test";
import { revisionSampling } from "../catalog.ts";
import { parsePlan } from "../plan.ts";
import { sourceHash } from "../workspace.ts";
import type { LocalRun } from "./model.ts";
import { comparePrepared } from "./runner.ts";

const id = "rust/u64/random/48/s1/api";
const hash = (bytes: string) => createHash("sha256").update(bytes).digest("hex");

/** Synthetic workers exercise frozen-build execution, never performance claims. */
async function fixture(t: TestContext) {
  const root = await mkdtemp(join(tmpdir(), "tessera-prepared-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(join(root, "bench/rust"), { recursive: true });
  await writeFile(join(root, "bench/rust/fixture.rs"), "// fixture\n");
  await writeFile(join(root, "bench/catalog.toml"), "fixture\n");
  const prepared = join(root, "prepared");
  const calls = join(root, "calls");
  const worker = `#!/usr/bin/env node
const fs = require('node:fs');
const id = ${JSON.stringify(id)};
fs.appendFileSync(${JSON.stringify(calls)}, 'worker\\n');
if (process.env.TESSERA_BENCH_LIST) { console.log(id); process.exit(0); }
require('node:readline').createInterface({input: process.stdin}).on('line', request => {
  const value = request === 'warm' ? {id, samples: ${revisionSampling(48).samples}}
    : request === 'sample' ? {elapsed_ns: 100, sorts: 1} : request === 'stop' ? {} : {id};
  process.stdout.write(JSON.stringify(value) + '\\n', () => {
    if (request === 'stop') process.exit(0);
  });
});
`;
  const build = {
    rustc: "fixture",
    cargo_encoded_rustflags: "fixture",
    target: "fixture",
    profile: "release",
    opt_level: "3",
    debug: "false",
    features: [],
  };
  const run: LocalRun = {
    schema: 1,
    protocol: "paired-samples-v2",
    baseline: "b".repeat(40),
    head: "a".repeat(40),
    dirty: false,
    source: {
      baseline: "b".repeat(64),
      current: "a".repeat(64),
      harness: await sourceHash(join(root, "bench/rust")),
      catalog: hash("fixture\n"),
    },
    binaries: { baseline: hash(worker), current: hash(worker) },
    binaryPaths: { baseline: "binaries/baseline/worker", current: "binaries/current/worker" },
    builds: { baseline: build, current: build },
    rustc: "fixture",
    flags: "fixture",
    configuration: "portable-simd",
    host: {
      platform: "build-host",
      release: "fixture",
      arch: "fixture",
      cpu: "build CPU",
      affinity: "inherited",
    },
    cases: [id],
  };
  for (const path of Object.values(run.binaryPaths!)) {
    await mkdir(join(prepared, path, ".."), { recursive: true });
    await writeFile(join(prepared, path), worker, { mode: 0o755 });
  }
  const plan = parsePlan({
    schema: 1,
    harness: "smoke",
    algorithms: ["tessera"],
    include: [{ types: ["u64"], distributions: ["random"], lengths: [48] }],
  });
  await writeFile(join(prepared, "plan.json"), JSON.stringify(plan));
  await writeFile(join(prepared, "local.json"), JSON.stringify(run));
  return { root, prepared, calls, run, worker, out: join(root, "result") };
}

test("prepared pairs execute full samples with retained binaries and the measurement host", async (t) => {
  const f = await fixture(t);
  await comparePrepared({ ...f, shard: { index: 1, count: 1 } });
  const measured: LocalRun = JSON.parse(await readFile(join(f.out, "local.json"), "utf8"));
  assert.equal(measured.samples![id].length, revisionSampling(48).samples);
  assert.equal(measured.host.platform, process.platform);
  assert.deepEqual(measured.shard?.allCases, [id]);
  assert.equal(await readFile(join(f.out, measured.binaryPaths!.current), "utf8"), f.worker);
  assert.deepEqual(JSON.parse(await readFile(join(f.prepared, "local.json"), "utf8")), f.run);
  assert.ok((await readFile(join(f.out, "index.md"), "utf8")).includes(id));
});

for (const damage of ["binary", "source", "escape", "measured", "basename"] as const) {
  test(`prepared pairs reject ${damage} before executing retained code`, async (t) => {
    const f = await fixture(t);
    const binary = join(f.prepared, f.run.binaryPaths!.current);
    if (damage === "binary") await writeFile(binary, `${f.worker}\n// changed`);
    if (damage === "source") await writeFile(join(f.root, "bench/catalog.toml"), "changed");
    if (damage === "escape") {
      const outside = join(f.root, "outside");
      await writeFile(outside, f.worker, { mode: 0o755 });
      await rm(binary);
      await symlink(outside, binary);
    }
    if (damage === "basename") {
      f.run.binaryPaths!.current = "binaries/current/different";
      await writeFile(join(f.prepared, f.run.binaryPaths!.current), f.worker, { mode: 0o755 });
      await writeFile(join(f.prepared, "local.json"), JSON.stringify(f.run));
    }
    if (damage === "measured") {
      f.run.samples = { [id]: [] };
      await writeFile(join(f.prepared, "local.json"), JSON.stringify(f.run));
    }
    await assert.rejects(comparePrepared(f));
    await assert.rejects(readFile(f.calls), { code: "ENOENT" });
  });
}
