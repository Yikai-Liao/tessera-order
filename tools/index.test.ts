// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { createProgram } from "./index.ts";
import { parseShard } from "./options.ts";

const cli = fileURLToPath(new URL("./index.ts", import.meta.url));

test("CLI exposes experiments separately from continuous stages and CI checks", () => {
  const program = createProgram();
  const group = (name: string) => program.commands.find((command) => command.name() === name)!;
  const compare = group("compare");
  assert.deepEqual(compare.commands.map((command) => command.name()).sort(), [
    "algorithms",
    "commit",
    "counters",
    "probe-counters",
    "resume",
    "revisions",
  ]);
  assert.deepEqual(
    group("continuous")
      .commands.map((command) => command.name())
      .sort(),
    ["matrix", "measure", "prepare", "submit"],
  );
  assert.ok(group("ci").commands.some((command) => command.name() === "check"));
  assert.ok(!program.commands.some((command) => command.name() === "check"));
  for (const command of compare.commands) {
    const options = command.options.map((option) => option.long);
    assert.ok(!options.includes("--shard"));
    assert.ok(!options.includes("--prepare-only"));
    assert.equal(options.includes("--algorithms"), command.name() === "algorithms");
    const plan = command.options.find((option) => option.long === "--plan");
    if (plan)
      assert.ok(
        plan.description.includes(
          ["commit", "counters"].includes(command.name())
            ? "local/continuous"
            : "smoke/release/parallel",
        ),
      );
  }
  const prepare = group("continuous").commands.find((command) => command.name() === "prepare")!;
  assert.equal(
    prepare.options.find((option) => option.long === "--plan")!.defaultValue,
    "continuous",
  );
});

test("CLI rejects misplaced options and incompatible plans before creating outputs", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-cli-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  for (const args of [
    ["compare", "commit", "--algorithms", "tessera"],
    ["compare", "counters", "--algorithms", "tessera"],
    ["compare", "revisions", "--algorithms", "tessera"],
    ["compare", "commit", "--prepare-only"],
    ["compare", "commit", "--shard", "1/4"],
    ["compare", "commit", "--plan", "smoke"],
    ["compare", "algorithms", "--plan", "continuous"],
    ["continuous", "prepare", "--plan", "release"],
    ["continuous", "measure", "missing", "--shard", "5/4"],
    ["continuous", "measure", "missing", "--shard", "1/65"],
  ]) {
    const result = spawnSync(process.execPath, [cli, ...args, "--out", join(directory, "result")], {
      encoding: "utf8",
    });
    assert.equal(result.status, 1, result.stderr);
    assert.equal(result.stdout, "");
    assert.deepEqual(await readdir(directory), []);
  }
  assert.deepEqual(parseShard("1/4"), { index: 1, count: 4 });
  for (const value of ["0/4", "1/0", "1/4junk", "1.5/4", "1/4/2"])
    assert.throws(() => parseShard(value));
});
