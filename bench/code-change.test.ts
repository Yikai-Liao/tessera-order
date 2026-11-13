// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdir, mkdtemp, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { codeChange, codeTable, productionLines } from "./code-change.ts";

test("tables include changed files even with zero net change and omit unchanged files", () => {
  const change = {
    baseHash: "a".repeat(40),
    hash: "b".repeat(40),
    baseline: 110,
    current: 110,
    added: 5,
    removed: 5,
    files: [
      { path: "engine.rs", baseline: 100, current: 100, added: 5, removed: 5 },
      { path: "block.rs", baseline: 10, current: 10, added: 0, removed: 0 },
    ],
  };
  const table = codeTable(change, { baseline: "main", current: "PR" });
  assert.match(table, /\| Core engine \| 110 \| 110 \| 5 \| 5 \| 0 \|/);
  assert.match(table, /\| `engine.rs` \| 100 \| 100 \| 5 \| 5 \| 0 \|/);
  assert.doesNotMatch(table, /block.rs/);
  assert.match(
    codeTable(change, { baseline: "main", current: "PR", details: true }),
    /\| `block.rs` \|/,
  );
});

test("production lines exclude nested comments and tests without interpreting literals", () => {
  const source = [
    "//! documentation",
    "/* outer /* nested */ comment */",
    "fn run<'a>(value: &'a str) { // comment",
    '    let text = r##"/* #[cfg(test)] { // */"##;',
    '    let byte = b\'{\'; let escaped = "\\" // }";',
    "    /* comment */ use_value(value);",
    "}",
    "#[cfg(",
    '    all(feature = "rayon", test),',
    ")]",
    "#[allow(dead_code)]",
    "fn helper() { assert!(true); }",
    "#[test]",
    "fn unit() { let a = [1, 2]; }",
    "#[cfg(test)] mod separate;",
    "#[cfg(test)] mod inline { #[test] fn nested() {} }",
    "",
  ].join("\n");
  const result = productionLines(source);
  assert.equal(result.lines.length, 5);
  assert.match(result.lines.join("\n"), /cfg\(test\)/);
  assert.doesNotMatch(result.lines.join("\n"), /documentation|comment|helper|unit|separate|inline/);
  assert.deepEqual(result.testModules, ["separate", "inline"]);
});

test("production cfg keeps feature alternatives and not(test), excluding test-only combinations", () => {
  const result = productionLines(
    [
      "#[cfg(not(test))] fn production() {}",
      '#[cfg(any(test, feature = "rayon"))] fn optional() {}',
      '#[cfg(all(any(test, test), feature = "rayon"))] fn test_only() {}',
      "#[cfg(not(not(test)))] fn double_negative() {}",
    ].join("\n"),
  );
  assert.equal(result.lines.length, 2);
  assert.match(result.lines.join("\n"), /production|optional/);
  assert.doesNotMatch(result.lines.join("\n"), /test_only|double_negative/);
  assert.deepEqual(productionLines("#![cfg(test)]\nfn helper() {}\n").lines, []);
});

test("multiline literals count as code and cannot terminate test bodies", () => {
  const result = productionLines(
    [
      'const TEXT: &str = r#"first',
      "// literal text",
      'last"#;',
      '#[cfg(test)] fn test_only() { let text = r#" } "#; }',
      "fn remaining() {}",
    ].join("\n"),
  );
  assert.equal(result.lines.length, 4);
  assert.match(result.lines.join("\n"), /\/\/ literal text/);
  assert.match(result.lines.join("\n"), /remaining/);
});

test("block-initialized test constants leave no production semicolon", () => {
  const result = productionLines(
    "#[cfg(test)]\nconst HELPER: [usize; 1] = { [1] };\nfn production() {}\n",
  );
  assert.deepEqual(result.lines, ["fn production ( ) { }"]);
});

test("unsupported test module rules decline the metric instead of miscounting", () => {
  for (const source of [
    "fn production() {}\nmod checks { #![cfg(test)] fn helper() {} }\nfn other() {}",
    '#[path = "other_checks.rs"] #[cfg(test)] mod checks;',
    '#[cfg(test)] mod checks { #[path = "../other_checks.rs"] mod helpers; }',
    '#![cfg(test)] #[path = "other_checks.rs"] mod checks;',
    "mod nested { #[cfg(test)] mod checks; }",
    "#[cfg_attr(test, ignore)] fn helper() {}",
  ])
    assert.throws(() => productionLines(source), /unsupported/);
  assert.deepEqual(productionLines("#[cfg(test)] pub(crate) mod checks;").testModules, ["checks"]);
});

test("exact commit comparison excludes tests, APIs and tooling, including additions and deletions", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "tessera-code-fixture-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const git = (...args: string[]) =>
    execFileSync(
      "git",
      [
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.com",
        "-c",
        "commit.gpgsign=false",
        "-c",
        "core.hooksPath=/dev/null",
        ...args,
      ],
      { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
    ).trim();
  const write = async (path: string, text: string) => {
    const target = join(root, path);
    await mkdir(dirname(target), { recursive: true });
    await writeFile(target, text);
  };
  const source = "rust/tessera-sort/src/";
  git("init", "-q");
  await write(
    `${source}engine.rs`,
    "// docs\nfn run() {\n    old();\n}\n#[cfg(test)] pub(crate) mod checks;\n",
  );
  await write(`${source}engine/checks.rs`, "fn test_helper() {}\n");
  await write(`${source}engine/plan.rs`, "fn retired() {}\n");
  await write(`${source}block.rs`, "pub fn block() {} // old comment\n");
  await write(`${source}key.rs`, "fn encoding() {}\n");
  await write(`${source}portable_simd.rs`, "fn simd_scan() {}\n");
  await write(`${source}scan.rs`, "fn scalar_scan() {}\n");
  await write(`${source}unwind.rs`, "fn recover() {}\n");
  await write(`${source}lib.rs`, "pub fn api() {}\n");
  await write(`${source}parallel.rs`, "pub fn parallel_api() {}\n");
  await write(`${source}trace.rs`, "fn figure_trace() {}\n");
  await write("bench/continuous/example.ts", "const tooling = 1;\n");
  git("add", ".");
  git("commit", "-qm", "baseline");
  const baseline = git("rev-parse", "HEAD");
  await write(
    `${source}engine.rs`,
    "// new docs\n\nfn run() {\n    new();\n    extra();\n}\n#[cfg(test)] pub(crate) mod checks;\n",
  );
  await write(`${source}engine/checks.rs`, "fn test_helper() {}\nfn extra_test() {}\n");
  await rm(join(root, source, "engine/plan.rs"));
  await write(`${source}block.rs`, "pub fn block() {} /* new nested /* comment */ */\n");
  await write(`${source}block/finish.rs`, "fn finish() {\n    complete();\n}\n");
  await write(`${source}key.rs`, "fn encoding() {}\nfn extra_encoding() {}\n");
  await write("bench/continuous/example.ts", "const tooling = 2;\n");
  git("add", ".");
  git("commit", "-qm", "current");
  const current = git("rev-parse", "HEAD");
  // Working-tree modifications must never enter a committed report.
  await write(
    `${source}engine.rs`,
    "fn unrelated_dirty_change() {}\n#[cfg(test)] pub(crate) mod checks;\n",
  );
  const result = await codeChange(baseline, current, root);
  assert.deepEqual(result, {
    baseHash: baseline,
    hash: current,
    baseline: 9,
    current: 13,
    added: 6,
    removed: 2,
    files: [
      { path: "block/finish.rs", baseline: 0, current: 3, added: 3, removed: 0 },
      { path: "engine.rs", baseline: 3, current: 4, added: 2, removed: 1 },
      { path: "engine/plan.rs", baseline: 1, current: 0, added: 0, removed: 1 },
      { path: "key.rs", baseline: 1, current: 2, added: 1, removed: 0 },
      ...["block.rs", "portable_simd.rs", "scan.rs", "unwind.rs"].map((path) => ({
        path,
        baseline: 1,
        current: 1,
        added: 0,
        removed: 0,
      })),
    ],
  });
  const unchanged = await codeChange("HEAD", "HEAD", root);
  assert.equal(unchanged.baseline, 13);
  assert.equal(unchanged.current, 13);
  assert.equal(unchanged.added, 0);
  assert.equal(unchanged.removed, 0);
  assert.ok(
    unchanged.files.every((file) => file.baseline === file.current && !file.added && !file.removed),
  );
  await assert.rejects(codeChange("missing", current, root));
  await assert.rejects(codeChange("f".repeat(40), current, root));

  await write(`${source}engine/new.rs`, "fn new_helper() {\n}\n");
  await write(`${source}engine/checks/helpers.rs`, "fn test_only_helper() {}\n");
  await write(
    `${source}key.rs`,
    "fn encoding() {}\nfn extra_encoding() {}\nfn staged_encoding() {}\n",
  );
  git("add", `${source}key.rs`);
  await rm(join(root, source, "scan.rs"));
  await write(".gitignore", `${source}engine/ignored.rs\n`);
  await write(`${source}engine/ignored.rs`, "fn ignored() {}\n");
  const working = await codeChange("HEAD", undefined, root);
  assert.equal(working.baseHash, current);
  assert.equal(working.hash, null);
  assert.equal(working.baseline, 13);
  assert.equal(working.current, 12);
  assert.equal(working.added, 4);
  assert.equal(working.removed, 5);
  assert.ok(!working.files.some((file) => /checks|ignored/.test(file.path)));

  const cli = fileURLToPath(new URL("../tools/index.ts", import.meta.url));
  const run = (...args: string[]) =>
    spawnSync(process.execPath, [cli, "code", ...args], {
      cwd: root,
      encoding: "utf8",
      timeout: 10000,
    });
  const json = run("--details");
  assert.equal(json.status, 0, json.stderr);
  assert.deepEqual(JSON.parse(json.stdout), { ...working, net: -1 });
  const text = run("--text");
  assert.equal(text.status, 0, text.stderr);
  assert.match(text.stdout, /\| Core engine \| 13 \| 12 \| 4 \| 5 \| −1 \|/);
  assert.doesNotMatch(text.stdout, /Scope:|No production-line changes/);
  const committed = run(baseline, "--current", "HEAD");
  assert.equal(committed.status, 0, committed.stderr);
  assert.equal(JSON.parse(committed.stdout).current, 13);
  assert.equal(JSON.parse(committed.stdout).net, 4);
  const invalid = run("missing");
  assert.equal(invalid.status, 1);
  assert.equal(invalid.stdout, "");

  await symlink(join(root, source, "key.rs"), join(root, source, "engine/link.rs"));
  await assert.rejects(codeChange("HEAD", undefined, root), /regular engine source/);
});
