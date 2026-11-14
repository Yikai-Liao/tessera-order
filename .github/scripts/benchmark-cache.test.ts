// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

test("PR preparation can restore the existing static main build's dependency cache", () => {
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
  ) as { environment: Record<string, string>; workspace: string; key: string }[];
  const [pr, main, native] = configurations;
  assert.deepEqual(pr, main);
  assert.equal(pr.environment.CARGO_PROFILE_BENCH_DEBUG, "0");
  assert.equal(pr.environment.CARGO_BUILD_TARGET, "x86_64-unknown-linux-gnu");
  assert.match(pr.environment.RUSTFLAGS, /target-feature=\+crt-static/);
  assert.doesNotMatch(native.environment.RUSTFLAGS, /crt-static/);
  assert.notEqual(native.key, pr.key);
});
