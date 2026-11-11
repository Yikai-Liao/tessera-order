// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
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
  const build = await retainBuild(directory, join(source, `compare${suffix}`));
  await writeFile(join(directory, "plan.json"), JSON.stringify(await loadPlan("smoke")));
  const session: Session = {
    schema: 2,
    platform: process.platform,
    planSha256: await fileHash(join(directory, "plan.json")),
    sampleMs: 20,
    memoryMib: 64,
    cpu: "0,2,4,6-8",
    group: "u64/random",
    build,
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
  assert.equal(await readFile(executables(directory).memory, "utf8"), "memory contents");
  const build = session.build;
  assert.equal(await isValidated(directory, build, session), false);
  await recordValidation(directory, build, session);
  assert.equal(await isValidated(directory, build, session), true);
  assert.equal(await isValidated(directory, build, { ...session, group: "u64/hot50" }), false);
  assert.equal(
    await isValidated(directory, { ...build, compareSha256: "0".repeat(64) }, session),
    false,
  );
});

test("resume rejects incomplete or altered experiments before execution", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-invalid-session-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  await assert.rejects(loadSession(directory), /preparation must finish/);
  const session = await prepared(directory);
  for (const path of [join(directory, "plan.json"), ...Object.values(executables(directory))]) {
    const original = await readFile(path);
    await writeFile(path, "changed");
    await assert.rejects(loadSession(directory), /changed/);
    await writeFile(path, original);
  }
  for (const invalid of [
    { ...session, schema: 1 },
    { ...session, platform: "different-platform" },
    { ...session, build: undefined },
    { ...session, build: { ...session.build, name: "portable-simd" } },
    { ...session, sampleMs: 0 },
  ]) {
    await writeFile(join(directory, "session.json"), JSON.stringify(invalid));
    await assert.rejects(loadSession(directory));
  }
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
