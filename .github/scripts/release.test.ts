// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import {
  readTagNotes,
  tagNotes,
  verifyPythonRelease,
  verifyRelease,
  verifyRustRelease,
} from "./release.ts";

const header = "object 0123\ntype commit\ntag rust/v0.2.0\ntagger A <a@b> 0 +0000\n\n";
const notes = "## Additions\n\n- `argsort` ([#225](https://example.test/225))\n";

test("Rust releases require a matching tag and a publishable crate", async (t) => {
  const root = mkdtempSync(join(tmpdir(), "tessera-rust-release-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, "rust/tessera-order"), { recursive: true });
  const manifest = join(root, "rust/tessera-order/Cargo.toml");
  writeFileSync(manifest, '[package]\nversion = "0.2.0"\n');
  const expected = { tag: "rust/v0.2.0", state: "verified" };
  assert.deepEqual(await verifyRustRelease(expected.tag, root), expected);
  assert.deepEqual(await verifyRelease(expected.tag, root), expected);
  for (const tag of ["rust/v0.3.0", "python/v0.2.0", "v0.2.0"])
    await assert.rejects(verifyRustRelease(tag, root), /Rust tag\/package version mismatch/);
  writeFileSync(manifest, '[package]\nversion = "0.2.0"\npublish = false\n');
  await assert.rejects(verifyRustRelease(expected.tag, root), /unpublished/);
  for (const tag of ["cpp/v0.2.0", "v0.2.0", "python/0.2.0", ""])
    await assert.rejects(verifyRelease(tag, root), /Unsupported release tag.*rust\/v.*python\/v/);
});

test("Python releases align package, adapter and embedded Rust core with the published Rust tag", async (t) => {
  const root = mkdtempSync(join(tmpdir(), "tessera-python-release-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const git = (...args: string[]) =>
    execFileSync(
      "git",
      [
        "-c",
        "user.name=A",
        "-c",
        "user.email=a@b",
        "-c",
        "commit.gpgSign=false",
        "-c",
        "tag.gpgSign=false",
        ...args,
      ],
      { cwd: root, encoding: "utf8" },
    ).trim();
  git("init", "-q");
  mkdirSync(join(root, "rust/tessera-order/src"), { recursive: true });
  mkdirSync(join(root, "python"));
  writeFileSync(join(root, "rust/tessera-order/Cargo.toml"), '[package]\nversion = "0.2.0"\n');
  writeFileSync(join(root, "rust/tessera-order/src/lib.rs"), "pub fn core() {}\n");
  const pyproject = join(root, "python/pyproject.toml");
  const adapter = join(root, "python/Cargo.toml");
  const validAdapter =
    '[package]\nversion = "0.2.0"\npublish = false\n[dependencies]\ntessera-order = { path = "../rust/tessera-order" }\n';
  writeFileSync(adapter, validAdapter);
  writeFileSync(pyproject, '[project]\nversion = "0.2.0"\n');
  git("add", ".");
  git("commit", "-q", "-m", "initial");
  await assert.rejects(verifyPythonRelease("python/v0.2.0", root), /Published Rust release tag/);
  git("tag", "rust/v0.2.0");
  // Adapter-only changes stay valid for synchronized and .postN releases.
  writeFileSync(join(root, "python/note.txt"), "adapter fix\n");
  git("add", "python/note.txt");
  git("commit", "-q", "-m", "adapter fix");
  for (const version of ["0.2.0", "0.2.0.post1", "0.2.0.post12"]) {
    writeFileSync(pyproject, `[project]\nversion = "${version}"\n`);
    const tag = `python/v${version}`;
    assert.deepEqual(await verifyPythonRelease(tag, root), { tag, state: "verified" });
    assert.deepEqual(await verifyRelease(tag, root), { tag, state: "verified" });
  }
  writeFileSync(pyproject, '[project]\nversion = "0.2.0.post1"\n');
  assert.deepEqual(
    await verifyRelease("python/v0.2.0.post1", root, {
      checkPublished: true,
      fetchIndex: async () => '{"vers":"0.1.0","yanked":false}\n{"vers":"0.2.0","yanked":false}\n',
    }),
    { tag: "python/v0.2.0.post1", state: "verified" },
  );
  await assert.rejects(
    verifyRelease("python/v0.2.0.post1", root, {
      checkPublished: true,
      fetchIndex: async () => '{"vers":"0.1.0","yanked":false}\n',
    }),
    /not published/,
  );
  await assert.rejects(
    verifyRelease("python/v0.2.0.post1", root, {
      checkPublished: true,
      fetchIndex: async () => '{"vers":"0.2.0","yanked":true}\n',
    }),
    /not published/,
  );
  await assert.rejects(
    verifyRelease("python/v0.2.0.post1", root, {
      checkPublished: true,
      fetchIndex: async () => {
        throw new Error("network unavailable");
      },
    }),
    /network unavailable/,
  );
  for (const version of [
    "0.3.0",
    "0.3.0.post1",
    "0.2.01",
    "0.2.0.post0",
    "0.2.0.post01",
    "0.2.0.post-1",
    "0.2.0.post1.dev1",
    "0.2.0.post1+local",
  ]) {
    writeFileSync(pyproject, `[project]\nversion = "${version}"\n`);
    await assert.rejects(
      verifyPythonRelease(`python/v${version}`, root),
      /Python project version must be/,
    );
  }
  writeFileSync(pyproject, '[project]\nversion = "0.2.0"\n');
  for (const tag of ["python/v0.3.0", "python/v0.2.0.post1", "rust/v0.2.0", "v0.2.0"])
    await assert.rejects(verifyPythonRelease(tag, root), /Python tag\/project version mismatch/);
  for (const metadata of [
    'version = "0.3.0"\npublish = false',
    'version = "0.2.0.post1"\npublish = false',
    'version = "0.2.0"\npublish = true',
    'version = "0.2.0"',
  ]) {
    writeFileSync(
      adapter,
      `[package]\n${metadata}\n[dependencies]\ntessera-order = { path = "../rust/tessera-order" }\n`,
    );
    await assert.rejects(
      verifyPythonRelease("python/v0.2.0", root),
      /Python Cargo package.version/,
    );
  }
  writeFileSync(
    adapter,
    '[package]\nversion = "0.2.0"\npublish = false\n[dependencies]\ntessera-order = "0.2.0"\n',
  );
  await assert.rejects(
    verifyPythonRelease("python/v0.2.0", root),
    /must point to \.\.\/rust\/tessera-order/,
  );
  writeFileSync(adapter, validAdapter);
  writeFileSync(pyproject, '[project]\nversion = "0.2.0.post1"\n');
  // Unreleased Rust edits (uncommitted or committed) must block a .postN release.
  writeFileSync(join(root, "rust/tessera-order/src/extra.rs"), "// untracked\n");
  await assert.rejects(
    verifyPythonRelease("python/v0.2.0.post1", root),
    /differs from published tag rust\/v0\.2\.0/,
  );
  rmSync(join(root, "rust/tessera-order/src/extra.rs"));
  writeFileSync(join(root, "rust/tessera-order/src/lib.rs"), "pub fn core() { let _ = 1; }\n");
  git("add", "rust/tessera-order/src/lib.rs");
  git("commit", "-q", "-m", "unreleased core edit");
  await assert.rejects(
    verifyPythonRelease("python/v0.2.0.post1", root),
    /differs from published tag rust\/v0\.2\.0/,
  );
});

test("tag notes drop the header and every git signature format", () => {
  assert.equal(tagNotes(`${header}${notes}`), notes);
  for (const kind of ["PGP SIGNATURE", "PGP MESSAGE", "SSH SIGNATURE", "SIGNED MESSAGE"])
    assert.equal(
      tagNotes(`${header}${notes}-----BEGIN ${kind}-----\nabc\n-----END ${kind}-----\n`),
      notes,
      kind,
    );
});

test("tag notes keep quoted markers and leading indentation", () => {
  const quoted = `${notes}\n\`\`\`\n-----BEGIN SSH SIGNATURE-----\n\`\`\`\n\n## Fixes\n\n- two\n`;
  const signature = "-----BEGIN SSH SIGNATURE-----\nabc\n-----END SSH SIGNATURE-----\n";
  assert.equal(tagNotes(`${header}${quoted}`), quoted);
  assert.equal(tagNotes(`${header}${quoted}${signature}`), quoted);
  const indented = "    cargo add tessera-order\n\n## Additions\n";
  assert.equal(tagNotes(`${header}${indented}`), indented);
});

test("tag notes reject empty messages and messages that lost every # line", () => {
  assert.throws(() => tagNotes(`${header}\n  \n`), /empty/);
  assert.throws(() => tagNotes("object 0123\ntype commit\n"), /empty/);
  assert.throws(() => tagNotes(`${header}- one\n`), /no line starting with #/);
  const crlf = "~~~sh\r\necho ready\r\n~~~\r\n\r\n## Fixes\r\n";
  assert.equal(tagNotes(`${header}${crlf}`), `${crlf.trimEnd()}\n`);
  const span = "```cargo add tessera-order```\n\n## Fixes\n";
  assert.equal(tagNotes(`${header}${span}`), span);
});

test("release notes come only from an annotated tag at the published commit", () => {
  const root = mkdtempSync(join(tmpdir(), "tessera-release-"));
  try {
    const git = (...args: string[]) =>
      execFileSync(
        "git",
        [
          "-c",
          "user.name=A",
          "-c",
          "user.email=a@b",
          "-c",
          "commit.gpgSign=false",
          "-c",
          "tag.gpgSign=false",
          ...args,
        ],
        { cwd: root, encoding: "utf8" },
      ).trim();
    git("init", "-q");
    git("commit", "-q", "--allow-empty", "-m", "first");
    const first = git("rev-parse", "HEAD");
    writeFileSync(join(root, "notes.md"), "## Additions\n\n- one\n");
    git("tag", "-a", "rust/v9.0.0", "--cleanup=whitespace", "-F", "notes.md");
    git("tag", "-a", "rust/v9.0.2", "-F", "notes.md");
    git("tag", "rust/v9.0.1");
    git("commit", "-q", "--allow-empty", "-m", "second");
    assert.equal(readTagNotes("rust/v9.0.0", root, first), "## Additions\n\n- one\n");
    assert.throws(() => readTagNotes("rust/v9.0.0", root, "HEAD"), /does not point at/);
    assert.throws(() => readTagNotes("rust/v9.0.1", root), /not an annotated tag/);
    // Git's default cleanup strips headings as comments.
    assert.throws(() => readTagNotes("rust/v9.0.2", root), /no line starting with #/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
