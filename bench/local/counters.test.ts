// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

test("container cleanup finishes before failure or cancellation escapes", {
  skip: process.platform === "win32",
}, async (t) => {
  const root = await mkdtemp(join(tmpdir(), "tessera-container-cleanup-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const calls = join(root, "calls");
  const removed = join(root, "removed");
  await writeFile(
    join(root, "docker"),
    `#!${process.execPath}
const fs = require("node:fs"), args = process.argv.slice(2);
fs.appendFileSync(${JSON.stringify(calls)}, JSON.stringify(args) + "\\n");
if (args[0] === "run") {
  if (process.env.TESSERA_TEST_DOCKER_MODE === "failure") process.exit(19);
  process.on("SIGTERM", () => {});
  process.kill(process.ppid, "SIGINT");
  setInterval(() => {}, 1000);
} else {
  setTimeout(() => {
    fs.writeFileSync(${JSON.stringify(removed)}, "removed");
    if (process.env.TESSERA_TEST_DOCKER_MODE === "cancel-cleanup-fails") process.exit(22);
  }, 30);
}
`,
    { mode: 0o755 },
  );
  const script = `
    import { existsSync } from "node:fs";
    import { runCounterContainer } from ${JSON.stringify(new URL("./counters.ts", import.meta.url).href)};
    import { CommandCancelledError } from ${JSON.stringify(new URL("../../tools/process.ts", import.meta.url).href)};
    try { await runCounterContainer(["fixture-image"]); }
    catch (error) {
      console.error(error.message);
      process.exitCode = !existsSync(${JSON.stringify(removed)}) ? 78 :
        error instanceof CommandCancelledError ? 77 : 79;
    }
  `;
  for (const mode of ["failure", "cancel", "cancel-cleanup-fails"]) {
    await writeFile(calls, "");
    await rm(removed, { force: true });
    const result = spawnSync(process.execPath, ["--input-type=module", "-e", script], {
      env: { ...process.env, PATH: `${root}:${process.env.PATH}`, TESSERA_TEST_DOCKER_MODE: mode },
      encoding: "utf8",
      timeout: 5000,
    });
    assert.equal(result.status, mode === "failure" ? 79 : 77, result.stderr);
    const invocations: string[][] = (await readFile(calls, "utf8"))
      .trim()
      .split("\n")
      .map((line) => JSON.parse(line));
    const name = invocations[0][2];
    assert.match(name, /^tessera-callgrind-[a-f\d-]+$/);
    assert.deepEqual(invocations, [
      ["run", "--name", name, "--rm", "fixture-image"],
      ["rm", "--force", name],
    ]);
    if (mode.startsWith("cancel")) assert.match(result.stderr, /cancelled by SIGINT/);
    if (mode === "cancel-cleanup-fails")
      assert.ok(result.stderr.includes(`Could not remove counter container ${name}`));
  }
});
