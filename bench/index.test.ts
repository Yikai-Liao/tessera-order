// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { decodeConfig } from "./config.ts";
import { createProgram } from "./index.ts";
import { parsePlan } from "./plan.ts";

test("plan exports round trip", () => {
  const cli = fileURLToPath(new URL("./index.ts", import.meta.url));
  const run = (...args: string[]) =>
    execFileSync(process.execPath, [cli, "plan", "local", ...args], { encoding: "utf8" });
  const json = parsePlan(decodeConfig(run("--format", "json"), "plan.json"));
  assert.deepEqual(parsePlan(decodeConfig(run(), "plan.toml")), json);
  assert.deepEqual(parsePlan(decodeConfig(run("--format", "toml"), "plan.toml")), json);
  assert.equal(JSON.parse(run("--schema")).type, "object");
});

test("commands reject invalid input before running", async () => {
  for (const args of [
    ["compare", "HEAD", "--native"],
    ["compare", "HEAD", "--filter", "["],
    ["release", "--suite", "invalid"],
    ["run", "--plan", "smoke", "--suite", "release"],
    ["run", "--lengths", "0"],
    ["plan", "--format", "yaml"],
    ["compare", "--types", "u64,"],
    ["report"],
    ["pr"],
    ["unrecognized"],
    ["release", "--cpu", "1,2"],
  ]) {
    const program = createProgram().configureOutput({ writeErr: () => {} });
    await assert.rejects(
      program.parseAsync(args, { from: "user" }),
      (error: unknown) =>
        error instanceof Error && "code" in error && String(error.code).startsWith("commander."),
    );
  }
});
