// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdir, mkdtemp, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { baselineArguments, validateApiListings } from "./runner.ts";
import { measuredCases, partition, runWorkers } from "./worker.ts";

const cli = fileURLToPath(new URL("../../tools/index.ts", import.meta.url));

test("build listing validation follows coverage growth rather than a fixed case count", () => {
  const legacy = ["rust/u64/random/10000000/s1/api"];
  for (const count of [1, 4, 8, 12]) {
    const parallel = Array.from(
      { length: count },
      (_, index) => `rust/u64/random/${10_000_000 + index}/s1/par-4`,
    );
    assert.deepEqual(validateApiListings([...legacy, ...parallel], legacy), {
      serial: legacy[0],
      parallel,
    });
    assert.throws(() => validateApiListings([...legacy, ...parallel, parallel[0]], legacy));
    assert.throws(() => validateApiListings([...legacy, ...parallel], [...legacy, ...legacy]));
  }
  assert.throws(() => validateApiListings(legacy, legacy));
  assert.throws(() => validateApiListings(["rust/u64/random/10000000/s1/par-2"], legacy));
});

test("baseline policy preserves main PR and stacked comparisons", () => {
  assert.ok(baselineArguments({}).includes("--thresholds-reset"));
  assert.ok(baselineArguments({}).includes("0.05"));
  assert.deepEqual(baselineArguments({ pr: "49" }), []);
  assert.deepEqual(baselineArguments({ pr: "49", startPoint: "pr-48", baseHash: "base" }), [
    "--start-point",
    "pr-48",
    "--start-point-hash",
    "base",
    "--start-point-clone-thresholds",
    "--start-point-reset",
  ]);
  assert.throws(() => baselineArguments({ startPoint: "main" }), /together/);
  assert.throws(() => baselineArguments({ baseHash: "base" }), /together/);
});

test("submission measures the second build after the first fails", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "tessera-submit-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const bin = join(root, "bin");
  await mkdir(bin);
  const calls = join(root, "calls");
  await writeFile(
    join(bin, "bencher"),
    `#!/usr/bin/env node
// SPDX-License-Identifier: BSD-2-Clause
const fs = require("node:fs");
const args = process.argv.slice(2);
fs.appendFileSync(process.env.BENCHER_CALLS, JSON.stringify(args) + "\\n");
if (args.includes("intel-v1-hybrid-v1")) {
  console.error("simulated measurement failure");
  process.exit(7);
}
console.log(JSON.stringify({ measured: true }));
`,
    { mode: 0o755 },
  );
  const out = join(root, "results");
  const result = spawnSync(
    process.execPath,
    [
      cli,
      "continuous",
      "submit",
      "--image",
      `registry/project@sha256:${"a".repeat(64)}`,
      "--hash",
      "b".repeat(40),
      "--project",
      "project",
      "--out",
      out,
      "--pr",
      "49",
      "--start-point",
      "pr-48",
      "--base-hash",
      "c".repeat(40),
    ],
    {
      encoding: "utf8",
      env: {
        ...process.env,
        PATH: `${bin}:${process.env.PATH}`,
        BENCHER_API_KEY: "test-secret",
        BENCHER_CALLS: calls,
      },
    },
  );
  assert.equal(result.status, 1);
  const outcomes = JSON.parse(await readFile(join(out, "outcomes.json"), "utf8"));
  assert.deepEqual(outcomes, { default: "failure", "portable-simd": "success" });
  assert.deepEqual(JSON.parse(result.stdout).result, outcomes);
  assert.ok(!result.stdout.includes("test-secret"));
  const commands = (await readFile(calls, "utf8"))
    .trim()
    .split("\n")
    .map((line) => JSON.parse(line) as string[]);
  assert.equal(commands.length, 2);
  assert.ok(
    commands.every(
      (args) =>
        args.includes("pr-49") &&
        args.includes("pr-48") &&
        args.includes("/bench/continuous/worker.ts") &&
        args.includes("--start-point-reset"),
    ),
  );
  assert.equal(
    JSON.parse(await readFile(join(out, "bencher-portable-simd.json"), "utf8")).measured,
    true,
  );
  assert.match(await readFile(`${out}.log`, "utf8"), /simulated measurement failure/);
});

test("scheduler balances API cases and excludes large cases from parallel lanes", () => {
  const cases = [
    "rust/u64/random/65536/s1/api",
    "rust/u64/equal/65536/s1/api",
    "rust/u64/reverse/65536/s1/api",
    "rust/u32/random/65536/s1/api",
    "rust/u128/random/65536/s1/api",
    "rust/u64/random/1000000/s1/api",
    "rust/u64/random/10000000/s1/api",
  ];
  const groups = partition(cases);
  assert.deepEqual(partition([...cases].reverse()), groups);
  assert.deepEqual(groups.large, [
    "rust/u64/random/1000000/s1/api",
    "rust/u64/random/10000000/s1/api",
  ]);
  assert.deepEqual(
    groups.small.map((lane) => lane.length),
    [3, 2],
  );
  const all = [...groups.small.flat(), ...groups.large];
  assert.equal(new Set(all).size, all.length);
  assert.deepEqual(new Set(all), new Set(cases));
});

test("supervisor rejects invalid IDs before starting workers", {
  skip: process.platform !== "linux",
}, async (t) => {
  const root = await mkdtemp(join(tmpdir(), "tessera-scheduler-ids-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const id = "rust/u64/random/65536/s1/api";
  const lists = ["", `${id}\n${id}`, id.replace("/api", "/workspace")];
  for (const [index, listed] of lists.entries()) {
    const binary = join(root, `bench-${index}`);
    const started = join(root, `started-${index}`);
    const out = join(root, `results-${index}`);
    await writeFile(
      binary,
      [
        "#!/usr/bin/env node",
        "// SPDX-License-Identifier: BSD-2-Clause",
        `if (process.env.TESSERA_BENCH_LIST) console.log(${JSON.stringify(listed)});`,
        `else require('node:fs').writeFileSync(${JSON.stringify(started)}, 'started');`,
      ].join("\n"),
      { mode: 0o755 },
    );
    const result = spawnSync(process.execPath, [cli, "continuous", "run", binary, "--out", out], {
      encoding: "utf8",
      timeout: 5000,
    });
    assert.equal(result.status, 1, result.stderr);
    assert.match(
      await readFile(`${out}.log`, "utf8"),
      /Expected distinct benchmark IDs|Invalid benchmark ID/,
    );
    await assert.rejects(readFile(started), { code: "ENOENT" });
  }
});

test("scheduler rejects duplicate and malformed measurements", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "tessera-cases-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  assert.deepEqual(await measuredCases([root]), new Set());
  const paths = ["0", "1"].map((lane) => join(root, lane));
  for (const path of paths) {
    await mkdir(join(path, "criterion/case/new"), { recursive: true });
    await writeFile(
      join(path, "criterion/case/new/benchmark.json"),
      JSON.stringify({ full_id: "same" }),
    );
  }
  await assert.rejects(measuredCases(paths), /duplicate/);
});

test("cloud scheduling runs through the public CLI without interleaving", {
  skip: process.platform !== "linux",
}, async (t) => {
  const root = await mkdtemp(join(tmpdir(), "tessera-scheduler-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const binary = join(root, "bench");
  await writeFile(
    binary,
    `#!/usr/bin/env node
// SPDX-License-Identifier: BSD-2-Clause
const fs = require("node:fs");
const path = require("node:path");
const cases = ["rust/u64/random/65536/s1/api", "rust/u32/random/65536/s1/api",
  "rust/u64/random/1000000/s1/api"];
if (process.env.TESSERA_BENCH_LIST) {
  console.log(cases.join("\\n"));
} else {
  for (const group of process.env.TESSERA_BENCH_GROUPS.split("\\n")) {
    for (const id of cases.filter(id => id.startsWith(group + "/"))) {
      const output = path.join(process.env.CRITERION_HOME, id, "new");
      fs.mkdirSync(output, { recursive: true });
      fs.writeFileSync(path.join(output, "benchmark.json"), JSON.stringify({ full_id: id }));
      console.log("record-start " + id);
      console.log("record-end " + id);
    }
  }
}
`,
    { mode: 0o755 },
  );
  const out = join(root, "results");
  const result = spawnSync(process.execPath, [cli, "continuous", "run", binary, "--out", out], {
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(JSON.parse(result.stdout).result, "completed");
  assert.deepEqual(await readdir(out), ["criterion.txt"]);
  const lines = (await readFile(join(out, "criterion.txt"), "utf8")).trim().split("\n");
  assert.equal(lines.length, 6);
  for (let i = 0; i < lines.length; i += 2)
    assert.equal(lines[i].replace("record-start", ""), lines[i + 1].replace("record-end", ""));
  assert.ok(lines.at(-1)!.includes("1000000"));
});

test("failed workers are reaped and their siblings are killed", {
  skip: process.platform !== "linux",
}, async (t) => {
  const root = await mkdtemp(join(tmpdir(), "tessera-worker-failure-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const allowed = (await readFile("/proc/self/status", "utf8")).match(
    /^Cpus_allowed_list:\s*(.+)$/m,
  )![1];
  const cpu = Number(allowed.split(/[,-]/)[0]);
  const binary = join(root, "bench");
  await writeFile(
    binary,
    `#!/usr/bin/env node
// SPDX-License-Identifier: BSD-2-Clause
const fs = require("node:fs");
if (process.env.TESSERA_BENCH_GROUPS === "fail") setTimeout(() => process.exit(1), 200);
else {
  fs.writeFileSync(${JSON.stringify(join(root, "pid"))}, String(process.pid));
  setInterval(() => {}, 1000);
}
`,
    { mode: 0o755 },
  );
  await assert.rejects(
    runWorkers(binary, [["fail/api"], ["hang/api"]], [cpu, cpu], join(root, "workers")),
  );
  const pid = Number(await readFile(join(root, "pid"), "utf8"));
  assert.throws(() => process.kill(pid, 0), { code: "ESRCH" });
});
