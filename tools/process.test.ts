// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
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

for (const graceful of [true, false]) {
  test(`cancellation reaps a child that ${graceful ? "exits zero" : "ignores termination"}`, {
    skip: process.platform === "win32",
  }, async (t) => {
    const root = await mkdtemp(join(tmpdir(), "tessera-command-cancel-"));
    t.after(() => rm(root, { recursive: true, force: true }));
    const pidPath = join(root, "pid");
    const worker = [
      `require("node:fs").writeFileSync(${JSON.stringify(pidPath)}, String(process.pid));`,
      `process.on("SIGTERM", () => { ${graceful ? "process.exit(0)" : ""} });`,
      'process.kill(process.ppid, "SIGINT");',
      "setInterval(() => {}, 1000);",
    ].join("\n");
    const script = [
      "import { command, CommandCancelledError } from " +
        JSON.stringify(new URL("./process.ts", import.meta.url).href) +
        ";",
      'try { await command(process.execPath, ["-e", ' +
        JSON.stringify(worker) +
        "], { capture: true });",
      'console.log("unexpected success"); } catch (error) { console.error(error.message);',
      "process.exitCode = error instanceof CommandCancelledError ? 1 : 2; }",
    ].join("\n");
    try {
      const result = spawnSync(process.execPath, ["--input-type=module", "-e", script], {
        encoding: "utf8",
        timeout: 5000,
      });
      assert.equal(result.status, 1, result.stderr);
      assert.equal(result.stdout, "");
      assert.match(result.stderr, /cancelled by SIGINT/);
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
