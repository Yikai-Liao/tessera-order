// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

for (const nested of [false, true]) {
  test(`cancellation kills logged descendants: nested=${nested}`, {
    skip: process.platform !== "linux",
  }, async (t) => {
    const root = await mkdtemp(join(tmpdir(), "tessera-descendant-cancel-"));
    t.after(() => rm(root, { recursive: true, force: true }));
    const pidPath = join(root, "pid");
    const grandchild = [
      'process.on("SIGTERM", () => {});',
      'require("node:fs").writeFileSync(process.env.PROBE_PID, String(process.pid));',
      'process.kill(Number(process.env.PROBE_SUPERVISOR), "SIGINT");',
      "setInterval(() => {}, 1000);",
    ].join("\n");
    const builder = join(root, "builder.mjs");
    await writeFile(
      builder,
      `import { command } from ${JSON.stringify(new URL("./process.ts", import.meta.url).href)};
     await command(process.execPath, ["-e", ${JSON.stringify(grandchild)}]);`,
    );
    const childArgs = nested ? [builder] : ["-e", grandchild];
    const parent = [
      'process.on("SIGTERM", () => process.exit(0));',
      `require("node:child_process").spawn(process.execPath, ${JSON.stringify(childArgs)}, { stdio: "inherit", env: { ...process.env, PROBE_SUPERVISOR: String(process.ppid) } });`,
      "setInterval(() => {}, 1000);",
    ].join("\n");
    const script = [
      `import { command, withCommandLog } from ${JSON.stringify(new URL("./process.ts", import.meta.url).href)};`,
      `try { await withCommandLog(${JSON.stringify(join(root, "log"))}, () => command(process.execPath, ["-e", ${JSON.stringify(parent)}], { env: { PROBE_PID: ${JSON.stringify(pidPath)} } }));`,
      "} catch (error) { console.error(error.message); process.exitCode = 1; }",
    ].join("\n");
    let pid: number | undefined;
    try {
      const result = spawnSync(process.execPath, ["--input-type=module", "-e", script], {
        encoding: "utf8",
        timeout: 5000,
      });
      assert.equal(result.status, 1, result.stderr);
      assert.match(result.stderr, /cancelled by SIGINT/);
      pid = Number(await readFile(pidPath, "utf8"));
      // Orphaned descendants can be zombies until init reaps them; neither a
      // zombie nor an absent process can keep competing with later measurements.
      let running = true;
      for (let attempt = 0; attempt < 100 && running; attempt++) {
        try {
          running = (await readFile(`/proc/${pid}/stat`, "utf8")).split(" ")[2] !== "Z";
        } catch (error) {
          assert.equal((error as NodeJS.ErrnoException).code, "ENOENT");
          running = false;
        }
        if (running) await new Promise((resolve) => setTimeout(resolve, 10));
      }
      assert.equal(running, false);
    } finally {
      if (pid) {
        try {
          process.kill(pid, "SIGKILL");
        } catch (error) {
          assert.equal((error as NodeJS.ErrnoException).code, "ESRCH");
        }
      }
    }
  });
}
