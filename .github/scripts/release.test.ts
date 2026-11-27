// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { readTagNotes, tagNotes } from "./release.ts";

const header = "object 0123\ntype commit\ntag rust/v0.2.0\ntagger A <a@b> 0 +0000\n\n";
const notes = "## Additions\n\n- `argsort` ([#225](https://example.test/225))\n";

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

test("tag notes reject empty messages and messages without a heading outside code", () => {
  assert.throws(() => tagNotes(`${header}\n  \n`), /empty/);
  assert.throws(() => tagNotes("object 0123\ntype commit\n"), /empty/);
  assert.throws(() => tagNotes(`${header}- one\n`), /no `#` heading/);
  assert.throws(() => tagNotes(`${header}\`\`\`sh\n# example\n\`\`\`\n`), /no `#` heading/);
  assert.equal(
    tagNotes(`${header}\`\`\`sh\n# example\n\`\`\`\n\n# Fixes\n`),
    "```sh\n# example\n```\n\n# Fixes\n",
  );
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
    assert.throws(() => readTagNotes("rust/v9.0.2", root), /no `#` heading/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
