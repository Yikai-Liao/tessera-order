// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmod, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { readCallgrind, reportCounters } from "./counters.ts";

test("counter reports retain clean, modified and legacy-unrecorded snapshot state", async (t) => {
  const out = await mkdtemp(join(tmpdir(), "tessera-counter-report-"));
  t.after(() => rm(out, { recursive: true, force: true }));
  const provenance = {
    rustc: "fixture",
    cargo_encoded_rustflags: "",
    target: "fixture",
    profile: "bench",
    opt_level: "3",
    debug: "false",
    features: [],
  };
  const id = "rust/u32/random/48/s1/api";
  for (const [dirty, label] of [
    [true, "modified working tree"],
    [false, "clean working tree"],
    [undefined, "working-tree state unrecorded"],
  ] as const) {
    const run = {
      schema: 1,
      kind: "callgrind-fixed-sorts",
      baseline: "base",
      head: "current",
      ...(dirty === undefined ? {} : { dirty }),
      source: {
        baseline: "base-source",
        current: "current-source",
        harness: "harness",
        catalog: "catalog",
      },
      builds: { baseline: provenance, current: provenance },
      binaries: { baseline: "base-binary", current: "current-binary" },
      host: { cpu: "fixture", kernel: "fixture" },
      valgrind: "fixture",
      sorts: 1,
      selected: [id],
      cases: [{ id, baseline: { Ir: 10 }, control: { Ir: 10 }, current: { Ir: 10 } }],
    } satisfies NonNullable<Parameters<typeof reportCounters>[1]>;
    await reportCounters(out, run);
    const initial = await readFile(join(out, "index.md"), "utf8");
    assert.ok(initial.includes(`Current head: current (${label})`));
    await writeFile(join(out, "counters.json"), JSON.stringify(run));
    await reportCounters(out);
    assert.equal(await readFile(join(out, "index.md"), "utf8"), initial);
  }
});

test("Callgrind summary preserves event names and omitted trailing zeroes", () => {
  assert.deepEqual(readCallgrind("events: Ir Dr Dw Bc Bcm\nsummary: 182 12 9\n"), {
    Ir: 182,
    Dr: 12,
    Dw: 9,
    Bc: 0,
    Bcm: 0,
  });
  for (const text of [
    "",
    "events: Ir Dr\nsummary: 0\n",
    "events: Ir Dr\nsummary: 1 -2\n",
    "events: Ir Ir\nsummary: 1 2\n",
    "events: Ir\nsummary: 1 2\n",
    "events: Dr\nsummary: 1\n",
    "events: Dr Ir\nsummary: 1\n",
  ])
    assert.throws(() => readCallgrind(text), /summary/);
});

test("PMU probe propagates operator cancellation instead of marking events unavailable", {
  skip: process.platform !== "linux",
}, async (t) => {
  const root = await mkdtemp(join(tmpdir(), "tessera-pmu-cancel-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const perf = join(root, "perf"),
    calls = join(root, "calls");
  await writeFile(
    perf,
    `#!/usr/bin/env node
    if (process.argv.includes("--version")) { console.log("perf test"); }
    else {
      require("node:fs").appendFileSync(${JSON.stringify(calls)}, "probe\\n");
      process.kill(process.ppid, "SIGINT");
      setInterval(() => {}, 1000);
    }
  `,
  );
  await chmod(perf, 0o755);
  const script = `
    import { probeCounters } from ${JSON.stringify(new URL("./counters.ts", import.meta.url).href)};
    import { CommandCancelledError } from ${JSON.stringify(new URL("../../tools/process.ts", import.meta.url).href)};
    try { await probeCounters(${JSON.stringify(join(root, "out"))}); }
    catch (error) { process.exitCode = error instanceof CommandCancelledError ? 77 : 78; }
  `;
  const result = spawnSync(process.execPath, ["--input-type=module", "-e", script], {
    env: { ...process.env, PATH: `${root}:${process.env.PATH}` },
    encoding: "utf8",
    timeout: 5000,
  });
  assert.equal(result.status, 77, result.stderr);
  assert.equal(await readFile(calls, "utf8"), "probe\n");
  await assert.rejects(readFile(join(root, "out/probe.json")), { code: "ENOENT" });
});

test("counter containers are forcibly removed before failures or cancellation escape", {
  skip: process.platform === "win32",
}, async (t) => {
  const root = await mkdtemp(join(tmpdir(), "tessera-container-cancel-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const docker = join(root, "docker"),
    calls = join(root, "calls");
  await writeFile(
    docker,
    `#!${process.execPath}
    const fs = require("node:fs"), args = process.argv.slice(2);
    fs.appendFileSync(${JSON.stringify(calls)}, JSON.stringify(args) + "\\n");
    if (args[0] === "run") {
      if (process.env.TESSERA_TEST_DOCKER_MODE.startsWith("cancel")) {
        process.on("SIGTERM", () => {});
        process.kill(process.ppid, "SIGINT");
        setInterval(() => {}, 1000);
      } else if (process.env.TESSERA_TEST_DOCKER_MODE === "failure") process.exit(19);
      else console.log("captured");
    } else if (process.env.TESSERA_TEST_DOCKER_MODE === "cancel-cleanup-fails") process.exit(22);
  `,
  );
  await chmod(docker, 0o755);
  const script = `
    import { runCounterContainer } from ${JSON.stringify(new URL("./counters.ts", import.meta.url).href)};
    import { CommandCancelledError } from ${JSON.stringify(new URL("../../tools/process.ts", import.meta.url).href)};
    try { console.log(await runCounterContainer(["fixture-image"])); }
    catch (error) { console.error(error.message); process.exitCode = error instanceof CommandCancelledError ? 77 : 79; }
  `;
  for (const mode of ["success", "failure", "cancel", "cancel-cleanup-fails"]) {
    await writeFile(calls, "");
    const result = spawnSync(process.execPath, ["--input-type=module", "-e", script], {
      env: { ...process.env, PATH: `${root}:${process.env.PATH}`, TESSERA_TEST_DOCKER_MODE: mode },
      encoding: "utf8",
      timeout: 5000,
    });
    const invocations: string[][] = (await readFile(calls, "utf8"))
      .trim()
      .split("\n")
      .map((line) => JSON.parse(line));
    const name = invocations[0][2];
    assert.match(name, /^tessera-callgrind-[a-f\d-]+$/);
    assert.deepEqual(invocations[0], ["run", "--name", name, "--rm", "fixture-image"]);
    if (mode === "success") {
      assert.equal(result.status, 0, result.stderr);
      assert.equal(result.stdout, "captured\n");
      assert.equal(invocations.length, 1);
    } else {
      assert.equal(result.status, mode === "failure" ? 79 : 77, result.stderr);
      assert.deepEqual(invocations[1], ["rm", "--force", name]);
      assert.equal(invocations.length, 2);
      if (mode.startsWith("cancel")) assert.match(result.stderr, /cancelled by SIGINT/);
      if (mode === "cancel-cleanup-fails")
        assert.ok(result.stderr.includes(`Could not remove counter container ${name}`));
    }
  }
});
