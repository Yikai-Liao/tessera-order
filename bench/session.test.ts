// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmod, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { loadPlan } from "./plan.ts";
import {
  executables,
  fileHash,
  isValidated,
  loadSession,
  recordValidation,
  retainBuild,
  type Session,
  withSessionLock,
} from "./session.ts";

/** Persist harmless executable bytes without invoking a compiler or benchmark. */
async function prepared(directory: string) {
  const source = join(directory, "build");
  await mkdir(source);
  const suffix = process.platform === "win32" ? ".exe" : "";
  await writeFile(join(source, `compare${suffix}`), "compare contents");
  await writeFile(join(source, `memory${suffix}`), "memory contents");
  const build = await retainBuild(directory, "default", join(source, `compare${suffix}`));
  await writeFile(join(directory, "plan.json"), JSON.stringify(await loadPlan("smoke")));
  const session: Session = {
    schema: 1,
    platform: process.platform,
    planSha256: await fileHash(join(directory, "plan.json")),
    sampleMs: 20,
    memoryMib: 64,
    cpu: "2",
    group: "u64/random",
    builds: [build],
  };
  await writeFile(join(directory, "session.json"), JSON.stringify(session));
  return session;
}

test("sessions retain immutable executable pairs and settings", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-session-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const expected = await prepared(directory);
  await rm(join(directory, "build"), { recursive: true });
  const session = await loadSession(directory);
  assert.deepEqual(session, expected);
  assert.equal(await readFile(executables(directory, "default").memory, "utf8"), "memory contents");
  const build = session.builds[0];
  assert.equal(await isValidated(directory, build, session), false);
  await recordValidation(directory, build, session);
  assert.equal(await isValidated(directory, build, session), true);
  assert.equal(await isValidated(directory, build, { ...session, group: "u64/hot50" }), false);
  assert.equal(
    await isValidated(directory, { ...build, compareSha256: "0".repeat(64) }, session),
    false,
  );
});

test("frozen selections do not depend on the current catalog", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-frozen-selection-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const session = await prepared(directory);
  // The retained executable, not the current checkout, interprets these identities.
  await writeFile(
    join(directory, "plan.json"),
    JSON.stringify({
      schema: 1,
      harness: "comparison",
      algorithms: ["retired-opponent"],
      include: [{ types: ["retired-type"] }],
    }),
  );
  session.planSha256 = await fileHash(join(directory, "plan.json"));
  await writeFile(join(directory, "session.json"), JSON.stringify(session));
  assert.deepEqual(await loadSession(directory), session);
});

test("resume rejects incomplete or altered experiments before execution", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-invalid-session-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  await assert.rejects(loadSession(directory), /preparation must finish/);
  const session = await prepared(directory);
  for (const path of [
    join(directory, "plan.json"),
    ...Object.values(executables(directory, "default")),
  ]) {
    const original = await readFile(path);
    await writeFile(path, "changed");
    await assert.rejects(loadSession(directory), /changed/);
    await writeFile(path, original);
  }
  for (const invalid of [
    { ...session, platform: "different-platform" },
    { ...session, builds: [] },
    { ...session, builds: [session.builds[0], session.builds[0]] },
    { ...session, sampleMs: 0 },
  ]) {
    await writeFile(join(directory, "session.json"), JSON.stringify(invalid));
    await assert.rejects(loadSession(directory));
  }
});

test("CLI resumes retained builds without overwriting logs or repeating validation", {
  skip: process.platform === "win32",
}, async (t) => {
  const root = await mkdtemp(join(tmpdir(), "tessera-session-cli-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const directory = join(root, "experiment");
  await mkdir(directory);
  const session = await prepared(directory);
  delete session.cpu;
  const binary = executables(directory, "default").compare;
  const calls = join(directory, "calls.jsonl");
  await writeFile(
    binary,
    `#!/usr/bin/env node
import { appendFileSync } from "node:fs";
appendFileSync(${JSON.stringify(calls)}, JSON.stringify(process.argv.slice(2)) + "\\n");
process.exit(process.argv[2] === "run" ? 42 : 0);
`,
  );
  await chmod(binary, 0o755);
  session.builds[0].compareSha256 = await fileHash(binary);
  await writeFile(join(directory, "session.json"), JSON.stringify(session));
  await writeFile(`${directory}.log`, "preparation log");
  const cli = fileURLToPath(new URL("../tools/index.ts", import.meta.url));
  const run = () =>
    spawnSync(process.execPath, [cli, "compare", "resume", directory], { encoding: "utf8" });
  for (let attempt = 0; attempt < 2; attempt++) {
    const result = run();
    assert.equal(result.status, 1, result.stderr);
    assert.equal(result.stdout, "");
    assert.match(result.stderr, /exited with 42; log:/);
  }
  const commands = (await readFile(calls, "utf8"))
    .trim()
    .split("\n")
    .map((line) => JSON.parse(line) as string[]);
  assert.deepEqual(
    commands.map((args) => args[0]),
    ["validate", "run", "run"],
  );
  assert.deepEqual(commands[1], [
    "run",
    "--plan",
    join(directory, "plan.json"),
    "--out",
    directory,
    "--group",
    "u64/random",
    "--sample-ms",
    "20",
    "--memory-mib",
    "64",
  ]);
  assert.equal(await readFile(`${directory}.log`, "utf8"), "preparation log");
  await writeFile(binary, "changed executable");
  const result = run();
  assert.equal(result.status, 1, result.stderr);
  assert.match(result.stderr, /Session executables changed/);
  assert.equal((await readFile(calls, "utf8")).trim().split("\n").length, 3);
});

test("session writes are exclusive and failures release the lock", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-session-lock-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  await assert.rejects(
    withSessionLock(directory, async () => {
      await assert.rejects(
        withSessionLock(directory, async () => assert.fail("second writer")),
        { code: "ELOCKED" },
      );
      throw new Error("measurement failed");
    }),
    /measurement failed/,
  );
  assert.equal(await withSessionLock(directory, async () => 42), 42);
});
