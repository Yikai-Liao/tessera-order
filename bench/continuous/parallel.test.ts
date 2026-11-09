// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { availableParallelism, tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { partition } from "./worker.ts";

const cli = fileURLToPath(new URL("../../tools/index.ts", import.meta.url));

test("parallel selection does not multiply or duplicate serial groups", () => {
  const api = "rust/u64/random/49/s1/api";
  const par = "rust/u64/random/49/s1/par-4";
  const selection = partition([api, par, "rust/u32/random/10000000/s1/par-4"]);
  assert.deepEqual(selection.small.flat(), [api]);
  assert.deepEqual(selection.large, []);
  assert.deepEqual(selection.parallel, [par, "rust/u32/random/10000000/s1/par-4"].sort());
});

test("four-thread cases exclude every other preparation and run one at a time", {
  skip: process.platform !== "linux" || availableParallelism() < 4,
}, async (t) => {
  const root = await mkdtemp(join(tmpdir(), "tessera-exclusive-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(join(root, "active"));
  const binary = join(root, "bench");
  const ids = [
    "rust/u64/random/49/s1/api",
    "rust/u32/random/49/s1/api",
    "rust/u64/random/10000000/s1/api",
    "rust/u64/random/10000000/s1/par-4",
    "rust/row_u32/random/10000000/s1/par-4",
  ];
  await writeFile(
    binary,
    `#!/usr/bin/env node
// SPDX-License-Identifier: BSD-2-Clause
const fs = require("node:fs"), path = require("node:path"), assert = require("node:assert/strict");
const ids = ${JSON.stringify(ids)}, root = ${JSON.stringify(root)};
if (process.env.TESSERA_BENCH_LIST) {
  assert.equal(process.env.TESSERA_BENCH_PARALLEL, "4");
  console.log(ids.join("\\n")); process.exit(0);
}
const selected = process.env.TESSERA_BENCH_IDS.split("\\n");
const parallel = selected.some(id => id.endsWith("/par-4"));
if (parallel) assert.equal(selected.length, 1, "one parallel case per process");
// A sibling may unlink its token after the directory snapshot.
const token = path.join(root, "active", (parallel ? "parallel-" : "serial-") + process.pid);
fs.writeFileSync(token, "");
const active = fs.readdirSync(path.join(root, "active"));
if (parallel) {
  assert.equal(active.length, 1, "parallel preparation overlapped other work");
  const allowed = fs.readFileSync("/proc/self/status", "utf8").match(/^Cpus_allowed_list:\\s*(.+)$/m)[1];
  const cpus = allowed.split(",").reduce((n, range) => {
    const [a, b = a] = range.split("-").map(Number); return n + b-a+1;
  }, 0);
  assert.equal(cpus, 4, "exactly four allowed CPUs");
} else {
  assert.ok(!active.some(name => name.startsWith("parallel-")));
}
fs.appendFileSync(path.join(root,"events"), "start " + selected.join(",") + "\\n");
Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 25);
for (const id of selected) {
  assert.ok(ids.includes(id));
  const output = path.join(process.env.CRITERION_HOME, id, "new");
  fs.mkdirSync(output, {recursive:true});
  fs.writeFileSync(path.join(output,"benchmark.json"), JSON.stringify({full_id:id}));
}
fs.appendFileSync(path.join(root,"events"), "end " + selected.join(",") + "\\n");
fs.unlinkSync(token);
`,
    { mode: 0o755 },
  );
  const out = join(root, "results");
  const result = spawnSync(process.execPath, [cli, "continuous", "run", binary, "--out", out], {
    encoding: "utf8",
  });
  assert.equal(
    result.status,
    0,
    result.stderr + (result.stdout ?? "") + (await readFile(`${out}.log`, "utf8").catch(String)),
  );
  const events = (await readFile(join(root, "events"), "utf8")).trim().split("\n");
  const firstParallel = events.findIndex((line) => line.includes("/par-4"));
  assert.ok(events.slice(firstParallel).every((line) => line.includes("/par-4")));
  const tail = events.slice(firstParallel);
  assert.equal(tail.length, 4);
  assert.equal(tail[0].replace("start ", ""), tail[1].replace("end ", ""));
  assert.equal(tail[2].replace("start ", ""), tail[3].replace("end ", ""));
});

test("insufficient CPUs fail before any benchmark work starts", {
  skip: process.platform !== "linux",
}, async (t) => {
  const root = await mkdtemp(join(tmpdir(), "tessera-exclusive-cpus-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const allowed = (await readFile("/proc/self/status", "utf8")).match(
    /^Cpus_allowed_list:\s*(.+)$/m,
  )![1];
  const cpus: number[] = [];
  for (const range of allowed.split(",")) {
    const [first, last = first] = range.split("-").map(Number);
    for (let cpu = first; cpu <= last && cpus.length < 2; cpu++) cpus.push(cpu);
    if (cpus.length === 2) break;
  }
  const binary = join(root, "bench");
  const started = join(root, "started");
  await writeFile(
    binary,
    `#!/usr/bin/env node
// SPDX-License-Identifier: BSD-2-Clause
if(process.env.TESSERA_BENCH_LIST) console.log("rust/u64/random/10000000/s1/par-4");
else require("node:fs").writeFileSync(${JSON.stringify(started)}, "bad");
`,
    { mode: 0o755 },
  );
  for (let count = 1; count <= cpus.length; count++) {
    const out = join(root, `results-${count}`);
    const result = spawnSync(
      "taskset",
      [
        "-c",
        cpus.slice(0, count).join(","),
        process.execPath,
        cli,
        "continuous",
        "run",
        binary,
        "--out",
        out,
      ],
      { encoding: "utf8" },
    );
    assert.equal(result.status, 1);
    assert.match(await readFile(`${out}.log`, "utf8"), /4 available CPUs/);
    await assert.rejects(readFile(started), { code: "ENOENT" });
  }
});
