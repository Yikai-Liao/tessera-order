// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { decodeConfig } from "../bench/config.ts";
import { parsePlan } from "../bench/plan.ts";
import { createProgram } from "./index.ts";

test("plan exports round trip", () => {
  const cli = fileURLToPath(new URL("./index.ts", import.meta.url));
  const run = (...args: string[]) =>
    execFileSync(process.execPath, [cli, "plan", "local", ...args], { encoding: "utf8" });
  const json = parsePlan(decodeConfig(run("--format", "json"), "plan.json"));
  assert.deepEqual(parsePlan(decodeConfig(run(), "plan.toml")), json);
  assert.equal(JSON.parse(run("--schema")).type, "object");
});

test("commands reject invalid input before running", async () => {
  for (const args of [
    ["compare", "commit", "HEAD", "--native"],
    ["compare", "commit", "HEAD", "--filter", "["],
    ["compare", "commit", "--layout-control", "cgu1"],
    ["compare", "commit", "--layout-seeds", "1", "--layout-control", "invalid"],
    ["compare", "commit", "--layout-seeds", "0,1"],
    ["compare", "commit", "--layout-seeds", "1,1"],
    ["compare", "commit", "--layout-seeds", "1,-2"],
    ["compare", "commit", "--layout-seeds", "1,"],
    ["compare", "algorithms", "--layout-seeds", "1,2"],
    ["compare", "counters", "--simd"],
    ["compare", "counters", "--sorts", "0"],
    ["continuous", "probe-counters"],
    ["continuous", "history"],
    ["compare", "algorithms", "--suite", "release"],
    ["compare", "algorithms", "--lengths", "0"],
    ["compare", "algorithms", "--sample-ms", "0"],
    ["compare", "algorithms", "--memory-mib", "1.5"],
    ["compare", "resume", "session", "--plan", "release"],
    ["compare", "resume", "session", "--cpu", "2"],
    ["compare", "resume"],
    ["compare", "algorithms", "--modes", "api"],
    ["compare", "commit", "--modes", "api"],
    ["plan", "--modes", "api"],
    ["plan", "--format", "yaml"],
    ["compare", "commit", "--types", "u64,"],
    ["report"],
    ["pr"],
    ["run"],
    ["release", "--suite", "invalid"],
    ["unrecognized"],
    ["status", "main", "--pr", "49"],
    ["status", "--pr", "0"],
    ["status", "--pr", "abc"],
    ["continuous", "unknown"],
    ["continuous", "build"],
    ["continuous", "run"],
    ["continuous", "submit"],
    ["compare", "algorithms", "--cpu", "2-1"],
    ["compare", "algorithms", "--cpu", "1,,2"],
  ]) {
    await assert.rejects(
      createProgram()
        .configureOutput({ writeErr: () => {} })
        .parseAsync(args, { from: "user" }),
    );
  }
});

test("task groups expose no legacy comparison aliases", () => {
  const program = createProgram();
  assert.deepEqual(
    program.commands.map((command) => command.name()),
    ["compare", "plan", "report", "check", "continuous", "ci", "native", "release", "status"],
  );
  assert.deepEqual(
    program.commands[0].commands.map((command) => command.name()),
    ["commit", "counters", "algorithms", "resume"],
  );
});
