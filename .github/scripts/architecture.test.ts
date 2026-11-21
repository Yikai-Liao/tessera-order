// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { checkRepository, checkRustSource } from "./check-architecture.ts";

const src = "rust/tessera-order/src";
const engine = `${src}/engine`;
const facade = `${engine}/mod.rs`;
const workspace = `${engine}/workspace.rs`;

function rejects(path: string, rule: string, sources: string[]) {
  for (const source of sources) {
    const diagnostics = checkRustSource(path, source);
    assert.ok(
      diagnostics.some(
        (message) => message.startsWith(`${path}:`) && message.includes(`[${rule}]`),
      ),
      source,
    );
  }
}

test("the repository satisfies the Rust architecture policies", async () => {
  assert.deepEqual(await checkRepository(), []);
});

test("the CLI checks untracked Rust source and skips deleted and ignored files", async () => {
  const root = await mkdtemp(join(tmpdir(), "tessera-architecture-"));
  const script = fileURLToPath(new URL("./check-architecture.ts", import.meta.url));
  const run = () => spawnSync(process.execPath, [script], { cwd: root, encoding: "utf8" });
  try {
    execFileSync("git", ["init", "-q"], { cwd: root });
    await mkdir(join(root, engine), { recursive: true });
    await writeFile(join(root, engine, "deleted.rs"), "#![allow(warnings)]");
    execFileSync("git", ["add", `${engine}/deleted.rs`], { cwd: root });
    await rm(join(root, engine, "deleted.rs"));
    await writeFile(join(root, ".gitignore"), "ignored.rs\n");
    await writeFile(join(root, engine, "ignored.rs"), "#![allow(warnings)]");
    const untracked = `${engine}/untracked.rs`;
    await writeFile(join(root, untracked), "#![allow(warnings)]");
    const invalid = run();
    assert.equal(invalid.status, 1, invalid.stderr);
    assert.match(invalid.stderr, /engine\/untracked\.rs:1:1 \[safety-lint-scope\]/);
    await rm(join(root, untracked));
    const valid = run();
    assert.equal(valid.status, 0, valid.stderr);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("the facade keeps modules private and names each export", () => {
  rejects(`${src}/lib.rs`, "engine-boundary", ["pub mod engine;", "pub(crate) mod r#engine;"]);
  const alternateFacade = `${src}/engine.rs`;
  rejects(alternateFacade, "engine-boundary", ["pub(crate) mod block;", "use super::algorithms;"]);
  rejects(facade, "engine-boundary", [
    "pub(crate) mod block;",
    "pub(crate) use self::tile::*;",
    "pub(crate) fn storage() {}",
  ]);
  assert.deepEqual(
    checkRustSource(
      facade,
      `
    pub(in crate::engine) mod block;
    pub(self) mod tile;
    pub mod thresholds;
    pub use key::RadixKey;
    pub(crate) use self::refine::{argsort, order_by_cached_indices, select_nth, sort};
    #[cfg(feature = "rayon")]
    pub(crate) use refine::parallel::sort as par_sort;
    #[cfg(feature = "trace")]
    pub use trace::{Event, partition as trace_partition};
  `,
    ),
    [],
  );
});

test("engine code cannot refer to adapters or import the crate root", () => {
  rejects(facade, "engine-boundary", ["use super::algorithms;"]);
  assert.equal(
    checkRustSource(`${engine}/tile.rs`, "fn f() { crate::algorithms::Adapter::<()>::run(); }")
      .length,
    1,
  );
  rejects(`${engine}/tile.rs`, "engine-boundary", [
    "use crate::algorithms::select_nth as select;",
    "use crate /* path */ :: { /* group */ r#algorithms as operations };",
    "fn f() { crate::algorithms::select_nth(); }",
    "fn f(_: crate::algorithms::Thing) {}",
    "type T = <crate::algorithms::Thing as Trait>::Assoc;",
    "fn f() { crate::engine::foo::<crate::algorithms::Thing>::bar(); }",
    "use super::super::algorithms;",
    "mod nested { use super::super::super::algorithms; }",
    "use crate as root;",
    "use crate::{self as root};",
    "use super::super::{self as root};",
    "use crate::*;",
  ]);
  assert.deepEqual(
    checkRustSource(
      `${engine}/tile/plan.rs`,
      `
    use ::std::ops;
    use super::super::key::Direct;
    use crate::engine::{Direct as Extractor, key};
    mod nested { use super::super::*; }
    // use crate::algorithms::sort;
    const NOTE: &str = "crate::algorithms::sort()";
  `,
    ),
    [],
  );
});

test("only the engine's top-level Workspace declaration is checked", () => {
  rejects(workspace, "engine-boundary", [
    "pub(crate) struct Workspace<T> { block: T }",
    "pub struct r#Workspace<T>(T);",
  ]);
  const localWorkspace = `
    mod nested {
      pub(crate) struct Workspace;
      impl Workspace {
        pub(crate) fn unrelated() {}
      }
    }
  `;
  assert.deepEqual(checkRustSource(`${engine}/refine.rs`, localWorkspace), []);
  assert.deepEqual(
    checkRustSource(
      workspace,
      `
      pub(in crate::engine) struct Workspace<T> { pub(super) block: T }
      ${localWorkspace}
    `,
    ),
    [],
  );
});

test("safety-lint exceptions can attach to a block's direct tail expression", () => {
  const path = `${engine}/block.rs`;
  for (const reason of ["reason", "r#reason"]) {
    assert.deepEqual(
      checkRustSource(
        path,
        `
      unsafe fn read_tail(p: *const u32) -> u32 {
        #[allow(unsafe_op_in_unsafe_fn, ${reason} = "caller guarantees readability")]
        p.read()
        // The comment does not change the tail expression.
      }
    `,
      ),
      [],
    );
  }
  rejects(path, "safety-lint-scope", [
    "unsafe fn f(p: *const u32) -> u32 { #[allow(unsafe_op_in_unsafe_fn)] p.read() }",
    'fn f() { #[allow(unsafe_op_in_unsafe_fn, reason = "local")] const VALUE: u32 = 1; }',
    'fn f() { #[allow(unsafe_op_in_unsafe_fn, reason = "local")] mod nested {} }',
  ]);
});

test("safety-lint exceptions need a reason and a function or statement scope", () => {
  const path = `${engine}/block.rs`;
  rejects(path, "safety-lint-scope", [
    "#![allow(r#warnings)]",
    "#![allow(rust_2024_compatibility)]",
    "#![allow(clippy::style)]",
    "#![allow(clippy::restriction)]",
    '#[allow(/* note */ clippy::r#all, reason = "fixture")] fn f() {}',
    '#![expect(clippy::undocumented_unsafe_blocks, reason = "fixture")]',
    "#[cfg_attr(unix, cfg_attr(test, allow /* note */ (clippy::missing_safety_doc)))] unsafe fn f() {}",
    '#[allow(unsafe_op_in_unsafe_fn, reason = "fixture")] mod movement {}',
    '#[allow(clippy::undocumented_unsafe_blocks, reason = r#" "#)] fn f() {}',
    "#[expect(clippy::missing_safety_doc)] unsafe fn f() {}",
  ]);
  assert.deepEqual(
    checkRustSource(
      path,
      `
    #![cfg_attr(not(feature = "trace"), allow(dead_code, unreachable_pub))]
    #[allow(missing_docs, reason = "the variants are documented")]
    enum Event { Done }
    #[cfg_attr(unix, expect(clippy::missing_safety_doc, reason = r#"private fixture"#))]
    #[cfg(test)]
    unsafe fn fixture() {
      #[allow(clippy::undocumented_unsafe_blocks, reason = "isolated fixture")]
      let _value = unsafe { load() };
    }
    #[allow(dead_code, reason = "contains warnings and allow(clippy::all)")]
    fn f() {}
    // #![allow(warnings)]
    const NOTE: &str = "#[allow(clippy::all)]";
  `,
    ),
    [],
  );
});
