// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const cli = fileURLToPath(new URL("./index.ts", import.meta.url));

for (const graceful of [true, false]) {
  test(`cancelled scheduler stops before large inputs: graceful=${graceful}`, {
    skip: process.platform !== "linux",
  }, async (t) => {
    const root = await mkdtemp(join(tmpdir(), "tessera-scheduler-cancel-"));
    t.after(() => rm(root, { recursive: true, force: true }));
    const binary = join(root, "bench");
    const calls = join(root, "calls");
    const pidPath = join(root, "pid");
    await writeFile(
      binary,
      [
        "#!/usr/bin/env node",
        "// SPDX-License-Identifier: BSD-2-Clause",
        'const fs = require("node:fs"), path = require("node:path");',
        'const cases = ["rust/u64/random/65536/s1/api", "rust/u64/random/1000000/s1/api"];',
        'if (process.env.TESSERA_BENCH_LIST) { console.log(cases.join("\\n")); process.exit(0); }',
        'const groups = process.env.TESSERA_BENCH_GROUPS.split("\\n");',
        `fs.appendFileSync(${JSON.stringify(calls)}, groups.join("\\n") + "\\n");`,
        'for (const id of cases.filter(id => groups.includes(id.slice(0, id.lastIndexOf("/"))))) {',
        ' const out = path.join(process.env.CRITERION_HOME, id, "new");',
        " fs.mkdirSync(out, { recursive: true });",
        ' fs.writeFileSync(path.join(out, "benchmark.json"), JSON.stringify({ full_id: id }));',
        "}",
        'if (groups.some(id => id.includes("/1000000/"))) process.exit(0);',
        `fs.writeFileSync(${JSON.stringify(pidPath)}, String(process.pid));`,
        `process.on("SIGTERM", () => { ${graceful ? "process.exit(0)" : ""} });`,
        'process.kill(process.ppid, "SIGINT");',
        "setInterval(() => {}, 1000);",
      ].join("\n"),
      { mode: 0o755 },
    );
    try {
      const result = spawnSync(
        process.execPath,
        [cli, "continuous", "run", binary, "--out", join(root, "results")],
        {
          encoding: "utf8",
          timeout: 5000,
        },
      );
      assert.equal(result.status, 1, result.stderr);
      assert.equal(result.stdout, "");
      assert.ok(!(await readFile(calls, "utf8")).includes("/1000000/"));
      const pid = Number(await readFile(pidPath, "utf8"));
      assert.throws(() => process.kill(pid, 0), { code: "ESRCH" });
    } finally {
      try {
        process.kill(Number(await readFile(pidPath, "utf8")), "SIGKILL");
      } catch (error) {
        assert.ok(["ENOENT", "ESRCH"].includes((error as NodeJS.ErrnoException).code ?? ""));
      }
    }
  });
}

test("cancelled submission never starts the second configuration", {
  skip: process.platform === "win32",
}, async (t) => {
  const root = await mkdtemp(join(tmpdir(), "tessera-submit-cancel-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const bin = join(root, "bin");
  await mkdir(bin);
  const calls = join(root, "calls");
  await writeFile(
    join(bin, "bencher"),
    [
      "#!/usr/bin/env node",
      "// SPDX-License-Identifier: BSD-2-Clause",
      `require("node:fs").appendFileSync(${JSON.stringify(calls)}, "called\\n");`,
      'process.on("SIGTERM", () => process.exit(0));',
      'process.kill(process.ppid, "SIGINT");',
      "setInterval(() => {}, 1000);",
    ].join("\n"),
    { mode: 0o755 },
  );
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
      join(root, "results"),
    ],
    {
      encoding: "utf8",
      timeout: 5000,
      env: { ...process.env, PATH: `${bin}:${process.env.PATH}`, BENCHER_API_KEY: "test-secret" },
    },
  );
  assert.equal(result.status, 1, result.stderr);
  assert.equal(result.stdout, "");
  assert.match(result.stderr, /cancelled by SIGINT/);
  assert.equal(await readFile(calls, "utf8"), "called\n");
});
