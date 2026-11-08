// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
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

test("documented custom algorithm coverage resolves without measuring", () => {
  const protocol = readFileSync(
    new URL("../docs/development/benchmarking.md", import.meta.url),
    "utf8",
  );
  const command = protocol.match(/^pnpm bench run --plan smoke --algorithms.*(?:\n {2}.*)*/m)?.[0];
  assert.ok(command, "Missing custom coverage example");
  const args = command
    .replace(/\\\n\s*/g, " ")
    .trim()
    .split(/\s+/)
    .slice(4);
  const cli = fileURLToPath(new URL("./index.ts", import.meta.url));
  const output = execFileSync(process.execPath, [cli, "plan", ...args, "--format", "json"], {
    encoding: "utf8",
  });
  const plan = parsePlan(decodeConfig(output, "plan.json"));
  assert.equal(plan.harness, "comparison");
  assert.deepEqual(plan.algorithms, ["tessera", "std-stable", "glidesort", "radsort"]);
  assert.deepEqual(plan.include, [
    { types: ["u64"], distributions: ["random", "zipf2"], lengths: [1234, 65536] },
  ]);
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
