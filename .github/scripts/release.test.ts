// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
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

test("Go release workflow retries the exact proxy version and reuses matching notes", () => {
  const directory = mkdtempSync(join(tmpdir(), "tessera-go-release-"));
  try {
    const workflow = readFileSync(new URL("../workflows/go-release.yml", import.meta.url), "utf8");
    const step = (name: string) => {
      const section = workflow.split(`      - name: ${name}\n`)[1].split("\n      - ")[0];
      return section
        .split("        run: |\n")[1]
        .split("\n")
        .filter((line) => line.startsWith("          "))
        .map((line) => line.slice(10))
        .join("\n");
    };
    const log = join(directory, "calls");
    const executable = (name: string, source: string) =>
      writeFileSync(join(directory, name), "#!/bin/sh\n" + source, { mode: 0o755 });
    executable(
      "gh",
      `printf '%s\\n' "$*" >> "$CALLS"
case "$2" in
view)
  [ "$EXISTING" = yes ] || exit 1
  printf '{"tagName":"go/tessera/v0.1.0","body":"%s","isDraft":false}\\n' "$NOTES";;
create) exit 0;;
*) exit 1;;
esac`,
    );
    executable(
      "go",
      `printf '%s\\n' "$*" >> "$CALLS"
case "$2" in
download)
  if [ ! -e tried ]; then touch tried; exit 1; fi
  printf '{"Path":"%s","Version":"%s","Sum":"h1:fixture"}\\n' "$MODULE" "$PROXY_VERSION";;
esac`,
    );
    executable("sleep", "exit 0");
    const module = "github.com/Yikai-Liao/tessera-order/go/tessera";
    writeFileSync(
      join(directory, "go-release.json"),
      JSON.stringify({ module, version: "v0.1.0" }),
    );
    writeFileSync(join(directory, "release-notes.md"), "## Release\n");
    const bash = (name: string, extra: Record<string, string>) =>
      execFileSync("bash", ["-ec", step(name)], {
        env: {
          ...process.env,
          PATH: `${directory}:${process.env.PATH}`,
          RUNNER_TEMP: directory,
          RELEASE_TAG: "go/tessera/v0.1.0",
          MODULE: module,
          PROXY_VERSION: "v0.1.0",
          CALLS: log,
          NOTES: "## Release",
          ...extra,
        },
        stdio: ["ignore", "pipe", "pipe"],
      });
    const publish = "Publish or verify the GitHub release";
    bash(publish, { EXISTING: "no" });
    bash(publish, { EXISTING: "yes" });
    assert.throws(() => bash(publish, { EXISTING: "yes", NOTES: "## Wrong" }));
    bash("Request indexing and verify the public module", {});
    const calls = readFileSync(log, "utf8");
    assert.equal(calls.match(/release create/g)?.length, 1);
    assert.equal(calls.match(/mod download/g)?.length, 2);
    assert.ok(calls.includes(`${module}@v0.1.0`));
    rmSync(join(directory, "go-release-consumer"), { recursive: true });
    assert.throws(() =>
      bash("Request indexing and verify the public module", { PROXY_VERSION: "v1.0.0" }),
    );
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
