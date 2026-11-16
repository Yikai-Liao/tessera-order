// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { checkArchitecture, repositorySources } from "./check-architecture.ts";

function check(sources: Record<string, string>) {
  return checkArchitecture(new Map(Object.entries(sources)));
}

function rejects(path: string, source: string, rule: string) {
  const diagnostics = check({ [path]: source });
  assert.ok(
    diagnostics.some((message) => message.startsWith(`${path}:`) && message.includes(`[${rule}]`)),
    `${source}\n${diagnostics.join("\n")}`,
  );
}

test("the maintained repository satisfies the architecture policies", async () => {
  assert.deepEqual(checkArchitecture(await repositorySources()), []);
});

test("the CLI checks untracked sources, ignores removed files and returns failure for violations", async () => {
  const root = await mkdtemp(join(tmpdir(), "tessera-architecture-"));
  const script = fileURLToPath(new URL("./check-architecture.ts", import.meta.url));
  const run = () => spawnSync(process.execPath, [script], { cwd: root, encoding: "utf8" });
  try {
    execFileSync("git", ["init", "-q"], { cwd: root });
    await mkdir(join(root, "tools"));
    await writeFile(join(root, "tools/removed.ts"), "export const old = 1;");
    execFileSync("git", ["add", "tools/removed.ts"], { cwd: root });
    await rm(join(root, "tools/removed.ts"));
    await writeFile(join(root, ".gitignore"), "ignored.ts\n");
    await writeFile(join(root, "tools/ignored.ts"), 'import { spawn } from "node:child_process";');
    await writeFile(
      join(root, "tools/task.ts"),
      'import { spawn as launch } from "node:child_process";',
    );
    const invalid = run();
    assert.equal(invalid.status, 1, invalid.stderr);
    assert.match(invalid.stderr, /tools\/task\.ts:1:1 \[command-entry\]/);
    await writeFile(join(root, "tools/task.ts"), "export const task = 1;");
    const valid = run();
    assert.equal(valid.status, 0, valid.stderr);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("locals in a different module cannot shadow restricted globals", () => {
  for (const [path, source, local, rule] of [
    ["bench/report/a.ts", 'fetch("url");', "const fetch = () => {};", "offline-report"],
    ["bench/plot/a.ts", "document.title;", "const document = {};", "plot-purity"],
    ["tools/task.ts", 'Bun.spawn(["tool"]);', "const Bun = {spawn() {}};", "command-entry"],
  ]) {
    const diagnostics = check({ [path]: source, "tools/local.ts": local });
    assert.ok(
      diagnostics.some(
        (message) => message.startsWith(`${path}:`) && message.includes(`[${rule}]`),
      ),
      diagnostics.join("\n"),
    );
  }
});

test("engine facade keeps movement modules and storage private", () => {
  const path = "rust/tessera-order/src/engine/mod.rs";
  for (const source of [
    "pub(crate) mod block;",
    "pub(super) mod workspace;",
    "pub(crate) use block::{Storage as Operation};",
    "pub(crate) use workspace::*;",
    "pub(crate) use self::tile::Scratch;",
    "pub(crate) fn storage() {}",
  ])
    rejects(path, source, "engine-boundary");
  assert.deepEqual(
    check({
      [path]: `
    mod block;
    pub(self) mod tile;
    pub mod thresholds;
    pub use key::RadixKey;
    pub(crate) use key::{Direct as Extractor, ByKey};
    pub(crate) use workspace::Workspace;
    #[cfg(feature = "trace")]
    pub use trace::{Event, partition as trace_partition};
  `,
    }),
    [],
  );
  rejects("rust/tessera-order/src/lib.rs", "pub mod engine;", "engine-boundary");
});

test("Workspace cannot expose movement storage through a field or getter", () => {
  const path = "rust/tessera-order/src/engine/workspace.rs";
  rejects(
    path,
    "pub(crate) struct Workspace<T> { pub(crate) block: block::Buffers<T> }",
    "engine-boundary",
  );
  rejects(
    path,
    "impl<T> Workspace<T> { pub(crate) fn buffers(&mut self) -> &mut block::Buffers<T> { &mut self.block } }",
    "engine-boundary",
  );
  assert.deepEqual(
    check({
      [path]: `
    pub(crate) struct Workspace<T> {
      pub(super) block: block::Buffers<T>,
      pub(in crate::engine) tile: tile::Scratch<T>,
    }
    impl<T> Workspace<T> {
      pub(crate) fn new() -> Self { todo!() }
      pub(super) fn internal(&self) {}
    }
  `,
    }),
    [],
  );
});

test("engine dependencies cannot point back at algorithm adapters", () => {
  const path = "rust/tessera-order/src/engine/tile.rs";
  for (const source of [
    "use crate::algorithms::select_nth as select;",
    "use crate /* why */ :: { /* why */ algorithms as operations };",
    "use crate::{cached as cache, engine::Direct};",
    "fn f() { crate::partition::partition(); }",
    "use super::super::parallel;",
    "mod nested { use super::super::super::r#algorithms; }",
    "use crate as root;",
    "use super::super as root; fn f() { root::algorithms::select_nth(); }",
    "use crate::*;",
  ])
    rejects(path, source, "engine-boundary");
  assert.deepEqual(
    check({
      [path]: `
    use super::key::Digit;
    use crate::engine::{ /* invocation-owned */ Direct, Workspace};
    // use crate::algorithms::sort;
    const NOTE: &str = "crate::partition::partition()";
  `,
    }),
    [],
  );
});

test("plotting accepts only presentation dependencies and private DOM objects", () => {
  assert.deepEqual(
    check({
      "bench/plot/chart.ts": `
      import * as Plot from "@observablehq/plot";
      import { parseHTML } from "linkedom/worker";
      import measure from "string-pixel-width";
      const { document: svgDocument } = parseHTML("<html></html>");
      const document = svgDocument;
      function render(fetch: () => void) { fetch(); return document; }
      export const options = { document: svgDocument };
    `,
    }),
    [],
  );
  for (const source of [
    'import { catalog } from "../catalog.ts";',
    'import type { PairedVerdict } from "../statistics.ts";',
    'import "node:fs";',
    'export * from "../continuous/chart.ts";',
    'const api = await import("../report/charts.ts");',
    'const name: import("../catalog.ts").CaseIdentity = value;',
    'const api = require("node:fs");',
    "const api = await import(moduleName);",
    'document.createElement("svg");',
    'const read = globalThis["fetch"]; read("url");',
    "const { localStorage: state } = globalThis; state.clear();",
    "const g = globalThis; const { document: dom } = g; dom.title;",
    "const seed = Math.random;",
    "const timestamp = Date.now();",
    "const { document } = globalThis; export const x = { document };",
  ])
    rejects("bench/plot/chart.ts", source, "plot-purity");
});

test("dependency closures catch re-exported side effects and tolerate local cycles", () => {
  const diagnostics = check({
    "bench/report/index.ts": 'export * from "./helper.ts";',
    "bench/report/helper.ts": 'export { build } from "../workspace.ts";',
    "bench/plot/chart.ts": 'export * from "./helper.ts";',
    "bench/plot/helper.ts": 'import { readFile } from "node:fs/promises";',
  });
  assert.ok(
    diagnostics.some(
      (message) =>
        message.includes("[offline-report]") &&
        message.includes("bench/report/index.ts -> bench/report/helper.ts"),
    ),
  );
  assert.ok(
    diagnostics.some(
      (message) =>
        message.includes("[plot-purity]") &&
        message.includes("bench/plot/chart.ts -> bench/plot/helper.ts"),
    ),
  );
  assert.deepEqual(
    check({
      "bench/plot/a.ts": 'export * from "./b.ts";',
      "bench/plot/b.ts": 'export * from "./a.ts";',
    }),
    [],
  );
});

test("offline reports may read and write results, but cannot run or publish", () => {
  assert.deepEqual(
    check({
      "bench/report/index.ts": `
      import { readFile, writeFile } from "node:fs/promises";
      import { join } from "node:path";
      import { median } from "../statistics.ts";
      export function f(fetch: () => void) { fetch(); }
    `,
      "bench/statistics.ts": 'import { catalog } from "./catalog.ts";',
      "bench/catalog.ts": 'import { readFileSync } from "node:fs";',
    }),
    [],
  );
  for (const source of [
    'import { command } from "../../tools/process.ts";',
    'export * from "../continuous/github.ts";',
    'import "node:https";',
    'import "undici";',
    'import { execFileSync } from "node:child_process";',
    'const request = fetch; request("url");',
    'new WebSocket("url");',
    'const { fetch: download } = globalThis; download("url");',
  ])
    rejects("bench/report/index.ts", source, "offline-report");
  rejects("bench/report/index.ts", 'export * from "./missing.ts";', "offline-report");
});

test("asynchronous subprocesses retain the shared execution owner", () => {
  for (const source of [
    'import { spawn as launch } from "node:child_process"; launch("tool");',
    'import * as cp from "child_process"; cp.exec("tool");',
    'export { execFile as run } from "node:child_process";',
    'const { spawn } = await import("node:child_process");',
    'const cp = require("child_process");',
    'import cp = require("node:child_process");',
    'import { spawn as start } from "bun";',
    'const run = Bun.spawn; run(["tool"]);',
    'const { spawnSync: run } = Bun; run(["tool"]);',
    'const runtime = globalThis.Bun; runtime["spawn"](["tool"]);',
    "const shell = Bun.$; shell`tool`;",
    'process.getBuiltinModule("child_process").spawn("tool");',
    'import { createRequire } from "node:module";',
  ])
    rejects("bench/local/runner.ts", source, "command-entry");
  assert.deepEqual(
    check({
      "tools/process.ts": 'import { spawn } from "node:child_process";',
      "tools/task.ts": `
      import { command } from "./process.ts";
      import { execFileSync as inspect } from "node:child_process";
      import type { ChildProcess } from "node:child_process";
      function local(Bun: {spawn(): void}) { Bun.spawn(); }
    `,
      "tools/process.test.ts": 'import { spawn } from "node:child_process";',
      "bench/fixtures/worker.ts": 'import { spawn } from "node:child_process";',
    }),
    [],
  );
});

test("production imports cannot use test helpers to bypass the subprocess policy", () => {
  const diagnostics = check({
    "tools/task.ts": 'import { launch } from "../bench/fixtures/worker.ts";',
    "bench/fixtures/worker.ts": 'import { spawn as launch } from "node:child_process";',
  });
  assert.ok(
    diagnostics.some(
      (message) =>
        message.startsWith("bench/fixtures/worker.ts:") && message.includes("[command-entry]"),
    ),
  );
  assert.deepEqual(
    check({
      "tools/task.ts": 'import type { Result } from "../bench/fixtures/worker.ts";',
      "bench/fixtures/worker.ts":
        'import { spawn } from "node:child_process"; export interface Result {}',
    }),
    [],
  );
});

test("production local imports use direct TypeScript source paths", () => {
  for (const specifier of [
    "../bench/fixtures/worker.js",
    "../bench/fixtures/worker",
    "../bench/fixtures/missing.ts",
  ]) {
    const diagnostics = check({
      "tools/task.ts": `import { launch } from "${specifier}";`,
      "bench/fixtures/worker.ts": 'import { spawn as launch } from "node:child_process";',
    });
    assert.ok(
      diagnostics.some(
        (message) => message.startsWith("tools/task.ts:") && message.includes("[command-entry]"),
      ),
      specifier,
    );
  }
  rejects("tools/task.ts", "const worker = await import(moduleName);", "command-entry");
});

test("safety-lint exceptions remain narrow and explain their reason", () => {
  const path = "rust/tessera-order/src/engine/block.rs";
  for (const source of [
    "#![allow(warnings)]",
    "#![allow(r#warnings)]",
    "#![allow(clippy::r#all)]",
    "#![allow(r#unsafe_op_in_unsafe_fn)]",
    '#[allow(/* note */ clippy::all, reason = "temporary")] fn f() {}',
    '#![expect(clippy::undocumented_unsafe_blocks, reason = "temporary")]',
    '#[cfg_attr(feature = "trace", allow(clippy::missing_safety_doc))] unsafe fn f() {}',
    '#[allow(unsafe_op_in_unsafe_fn, reason = "temporary")] mod movement {}',
    '#[allow(clippy::undocumented_unsafe_blocks, reason = " ")] fn f() {}',
    '#[allow(clippy::undocumented_unsafe_blocks, reason = r#" "#)] fn f() {}',
    "#[expect(clippy::missing_safety_doc)] unsafe fn f() {}",
  ])
    rejects(path, source, "safety-lint-scope");
  assert.deepEqual(
    check({
      [path]: `
    #![cfg_attr(not(feature = "trace"), allow(dead_code, unreachable_pub))]
    #[allow(missing_docs, reason = "each variant documents its fields")]
    enum Event { Done }
    #[expect(clippy::missing_safety_doc, reason = "private fixture")]
    unsafe fn fixture() {}
    #[expect(clippy::missing_safety_doc, reason = r#"private fixture"#)]
    unsafe fn raw_reason_fixture() {}
    #[allow(dead_code, reason = "contains ,warnings, and allow(clippy::all)")]
    fn f() {}
    // #![allow(warnings)]
    const NOTE: &str = "#[allow(clippy::all)]";
  `,
    }),
    [],
  );
});
