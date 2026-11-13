// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { codeChange, codeTable } from "./code-change.ts";

test("shared code statistics exclude comments and tests and support commits and the working tree", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "tessera-code-test-"));
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
  // The baseline predates the crate rename; the comparison matches files across it.
  let source = join(root, "rust/tessera-sort/src");
  await mkdir(join(source, "engine"), { recursive: true });
  const write = (name: string, text: string) => writeFile(join(source, name), text);
  const engine =
    '/* outer /* nested */ comment */\nfn run() {\n let text = r#"// literal"#;\n use_key();\n}\n#[cfg(all(feature="rayon", test))] fn helper() {}\n#[cfg(test)] pub(crate) mod checks;\n';
  await write("engine.rs", engine);
  await write("engine/checks.rs", "fn test_helper() {}\n");
  for (const name of ["key", "block", "portable_simd", "unwind", "lib", "parallel", "trace"])
    await write(`${name}.rs`, `fn ${name}() {}\n`);
  git("init", "-q");
  git("add", ".");
  git("commit", "-qm", "baseline");
  const base = git("rev-parse", "HEAD");
  git("mv", "rust/tessera-sort", "rust/tessera-order");
  source = join(root, "rust/tessera-order/src");
  await write("engine.rs", engine.replace(" use_key();", " use_key();\n extra();"));
  await write("scan.rs", "fn scan() {}\n");
  await rm(join(source, "block.rs"));
  git("add", ".");
  git("commit", "-qm", "current");
  const change = await codeChange(base, "HEAD", root);
  assert.deepEqual([change.baseline, change.current, change.added, change.removed], [8, 9, 2, 1]);
  assert.ok(!change.files.some((file) => /checks|lib\.rs|parallel\.rs|trace\.rs/.test(file.path)));
  const table = codeTable(change, { baseline: "main", current: "PR" });
  assert.match(table, /`engine.rs`/);
  assert.doesNotMatch(table, /`key.rs`/);
  // Replacements remain visible even when the file's total does not change.
  const replaced = {
    ...change,
    files: [{ path: "key.rs", baseline: 1, current: 1, added: 1, removed: 1 }],
  };
  assert.match(codeTable(replaced, { baseline: "main", current: "PR" }), /`key.rs`/);
  await write("scan.rs", "fn scan() {}\nfn dirty() {}\n");
  const cli = fileURLToPath(new URL("../tools/index.ts", import.meta.url));
  const result = spawnSync(process.execPath, [cli, "code", "--details"], {
    cwd: root,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);
  const working = JSON.parse(result.stdout);
  assert.deepEqual(
    [working.baseline, working.current, working.net, working.hash],
    [9, 10, 1, null],
  );
  assert.equal((await codeChange("HEAD", "HEAD", root)).current, 9);
  await writeFile(join(root, ".gitattributes"), "rust/tessera-order/src/engine.rs export-ignore\n");
  git("add", ".gitattributes");
  git("commit", "-qm", "archive excludes source");
  await assert.rejects(codeChange("HEAD", "HEAD", root), /ENOENT/);
  git("reset", "--hard", "HEAD^");
  await rm(join(root, "rust/tessera-order"), { recursive: true });
  assert.equal((await codeChange("HEAD", undefined, root)).current, 0);
});

test("figure trace instrumentation is not production code", async () => {
  const { production } = await import("./code-source.ts");
  const source =
    "use crate::trace::{self, Event};\nuse crate::key;\nfn run() {\n trace::emit(|| Event::Begin {\n len: 1,\n });\n work();\n trace::emit(|| Event::Final { physical })\n}\n";
  assert.deepEqual(production(source).lines, [
    "use crate :: key ;",
    "fn run ( ) {",
    "work ( ) ;",
    "}",
  ]);
});
