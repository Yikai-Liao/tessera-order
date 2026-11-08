// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { createProgram } from "./index.ts";

test("commands reject invalid input before running", async () => {
  for (const args of [
    ["compare", "HEAD", "--native"],
    ["compare", "HEAD", "--filter", "["],
    ["release", "--suite", "invalid"],
    ["run", "--plan", "smoke", "--suite", "release"],
    ["run", "--lengths", "0"],
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
