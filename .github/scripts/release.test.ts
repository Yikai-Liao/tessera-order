// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { readTagNotes, tagNotes } from "./release.ts";

const header = "object 0123\ntype commit\ntag rust/v0.2.0\ntagger A <a@b> 0 +0000\n\n";

test("tag notes keep Markdown headings and drop the header and signature", () => {
  const notes = "## Additions\n\n- `argsort` ([#225](https://example.test/225))\n";
  const signature = "-----BEGIN SSH SIGNATURE-----\nabc\n-----END SSH SIGNATURE-----\n";
  assert.equal(tagNotes(`${header}${notes}`), notes);
  assert.equal(tagNotes(`${header}${notes}\n${signature}`), notes);
  assert.throws(() => tagNotes(`${header}\n  \n`), /empty/);
  assert.throws(() => tagNotes("object 0123\ntype commit\n"), /empty/);
});

test("release notes come only from annotated tags", () => {
  const root = mkdtempSync(join(tmpdir(), "tessera-release-"));
  try {
    const git = (...args: string[]) =>
      execFileSync("git", ["-c", "user.name=A", "-c", "user.email=a@b", ...args], { cwd: root });
    git("init", "-q");
    git("commit", "-q", "--allow-empty", "-m", "init");
    writeFileSync(join(root, "notes.md"), "## Additions\n\n- one\n");
    git("tag", "-a", "rust/v9.0.0", "--cleanup=verbatim", "-F", "notes.md");
    git("tag", "rust/v9.0.1");
    assert.equal(readTagNotes("rust/v9.0.0", root), "## Additions\n\n- one\n");
    assert.throws(() => readTagNotes("rust/v9.0.1", root), /not an annotated tag/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
