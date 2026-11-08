// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { command, withCommandLog } from "./process.ts";

test("captured failures retain stdout diagnostics without polluting public stdout", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "tessera-command-log-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const out = join(root, "result");
  const invocation = `command(process.execPath, ["-e",
    "console.log('stdout-only diagnostic'); process.exit(7)"], { capture: true })`;
  for (const logged of [true, false]) {
    const script = `
      import { command } from ${JSON.stringify(new URL("./process.ts", import.meta.url).href)};
      import { runTask } from ${JSON.stringify(new URL("./tasks.ts", import.meta.url).href)};
      try {
        ${logged ? `await runTask(${JSON.stringify(out)}, false, () => ${invocation})` : `await ${invocation}`}
      } catch (error) {
        console.error(JSON.stringify({ error: error.message }));
        process.exitCode = 1;
      }`;
    const result = spawnSync(process.execPath, ["--input-type=module", "-e", script], {
      encoding: "utf8",
    });
    assert.equal(result.status, 1);
    assert.equal(result.stdout, "");
    assert.match(result.stderr, /exited with 7/);
    if (logged) {
      assert.ok(result.stderr.includes(`${out}.log`));
      assert.ok(!result.stderr.includes("stdout-only diagnostic"));
      assert.match(await readFile(`${out}.log`, "utf8"), /stdout-only diagnostic/);
    } else assert.match(result.stderr, /stdout-only diagnostic/);
  }
});

test("successful captures remain available for parsing without being logged", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "tessera-command-success-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const path = join(root, "commands.log");
  const result = await withCommandLog(path, () =>
    command(process.execPath, ["-e", "console.log('captured result')"], { capture: true }),
  );
  assert.equal(result, "captured result");
  assert.equal(await readFile(path, "utf8"), "");
});
