// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { codeChange, productionLines } from "./code-change.ts";

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
  await write(`${source}engine.rs`, "fn unrelated_dirty_change() {}\n");
  const result = await codeChange(baseline, current, root);
  assert.deepEqual(result, {
    baseline: 5,
    current: 8,
    added: 5,
    removed: 2,
    files: [
      { path: "block/finish.rs", added: 3, removed: 0 },
      { path: "engine.rs", added: 2, removed: 1 },
      { path: "engine/plan.rs", added: 0, removed: 1 },
    ],
  });
  assert.deepEqual(await codeChange(current, current, root), {
    baseline: 8,
    current: 8,
    added: 0,
    removed: 0,
    files: [],
  });
  await assert.rejects(codeChange("missing", current, root), /exact source commits/);
  await assert.rejects(codeChange("f".repeat(40), current, root));
});
