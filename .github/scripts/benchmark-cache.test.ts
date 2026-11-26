// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

test("PR and main static tracking share alignment flags and cache while native stays separate", () => {
  // Bun owns YAML parsing; the subprocess keeps Bun-only APIs out of tooling's Node types.
  const configurations = JSON.parse(
    execFileSync(
      process.execPath,
      [
        "-e",
        String.raw`const ci = Bun.YAML.parse(await Bun.file(".github/workflows/ci.yml").text());
const tracking = Bun.YAML.parse(await Bun.file(".github/workflows/main-benchmarks.yml").text());
function configuration(job, native) {
  const resolve = value => typeof value !== "string" ? value : value.replace(
    /\$\{\{(.*?)\}\}/g,
    (_, expression) => new Function("matrix", "github", "hashFiles", "return (" + expression + ")")(
      { native }, { workspace: "/workspace" }, (...paths) => JSON.stringify(paths),
    ),
  );
  const cache = job.steps.find(step => step.uses === "Swatinem/rust-cache@v2").with;
  return {
    environment: Object.fromEntries(Object.entries(job.env)
      .filter(([key]) => /^(CARGO|RUST|CC|CFLAGS|CXX|CMAKE|TESSERA_BENCH_TARGET_DIR)/.test(key))
      .map(([key, value]) => [key, resolve(value)])),
    workspace: cache.workspaces,
    key: resolve(cache["shared-key"]),
    saveIf: cache["save-if"] ?? null,
  };
}
console.log(JSON.stringify([
  configuration(ci.jobs.prepare, false),
  configuration(tracking.jobs.build, false),
  configuration(tracking.jobs.build, true),
]));`,
      ],
      { cwd: fileURLToPath(new URL("../..", import.meta.url)), encoding: "utf8" },
    ),
  ) as {
    environment: Record<string, string>;
    workspace: string;
    key: string;
    saveIf: string | boolean | null;
  }[];
  const [pr, main, native] = configurations;
  assert.equal(pr.workspace, main.workspace);
  assert.equal(pr.key, main.key);
  assert.deepEqual(pr.environment, main.environment);
  assert.match(pr.environment.RUSTFLAGS, /-x86-branches-within-32B-boundaries/);
  assert.match(pr.environment.RUSTFLAGS, /-align-all-functions=6/);
  assert.equal(pr.environment.CARGO_PROFILE_BENCH_DEBUG, "0");
  assert.equal(pr.environment.CARGO_BUILD_TARGET, "x86_64-unknown-linux-gnu");
  assert.match(pr.environment.RUSTFLAGS, /target-feature=\+crt-static/);
  assert.equal(pr.saveIf, "${{ env.PUBLISH == 'true' && vars.MAIN_BENCHMARKS_ENABLED != 'true' }}");
  assert.ok(
    main.saveIf === null || main.saveIf === true || main.saveIf === "true",
    `main static tracking must allow cache saves, got ${JSON.stringify(main.saveIf)}`,
  );
  assert.equal(native.environment.RUSTFLAGS, "");
  assert.notEqual(main.key, native.key);
});
