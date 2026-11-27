// SPDX-License-Identifier: BSD-2-Clause

import { execFileSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { parse } from "smol-toml";

/** Verify the Rust release tag without publishing or requiring registry credentials. */
export async function verifyRustRelease(tag: string, root: string) {
  const manifest = parse(await readFile(join(root, "rust/tessera-order/Cargo.toml"), "utf8"));
  const pkg = manifest.package as { version: string; publish?: boolean };
  if (tag !== `rust/v${pkg.version}` || pkg.publish === false)
    throw new Error("Rust tag/package version mismatch or package is unpublished");
  return { tag, state: "verified" };
}

/**
 * Release notes are the message of the annotated release tag, so publication
 * needs no manual step. Reject a lightweight tag, a tag that does not point at
 * the commit being published, and an unusable message before anything is
 * published.
 */
export function readTagNotes(tag: string, cwd?: string, commit?: string): string {
  const git = (...args: string[]) =>
    execFileSync("git", args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
  const ref = `refs/tags/${tag}`;
  if (git("cat-file", "-t", ref) !== "tag")
    throw new Error(`${tag} is not an annotated tag; its message holds the release notes`);
  if (
    commit !== undefined &&
    git("rev-parse", `${ref}^{commit}`) !== git("rev-parse", `${commit}^{commit}`)
  )
    throw new Error(`${tag} does not point at the commit being published`);
  return tagNotes(git("cat-file", "tag", ref));
}

// Git's signature markers (`gpg-interface.c`). Like git, cut at the last one,
// so notes that quote a marker keep their text.
const signatureStart =
  /^-----BEGIN (?:PGP SIGNATURE|PGP MESSAGE|SSH SIGNATURE|SIGNED MESSAGE)-----$/gm;

/** Extract the message of a raw tag object (`git cat-file tag`), without its signature. */
export function tagNotes(tagObject: string): string {
  const headerEnd = tagObject.indexOf("\n\n");
  const message = headerEnd < 0 ? "" : tagObject.slice(headerEnd + 2);
  const signature = [...message.matchAll(signatureStart)].at(-1)?.index ?? message.length;
  const notes = message.slice(0, signature).trim();
  if (notes === "") throw new Error("release tag message is empty; it holds the release notes");
  // The checklist groups notes under headings; none means git's default
  // `--cleanup=strip` removed them as comments.
  if (!/^#{1,6} /m.test(notes))
    throw new Error("release notes have no Markdown heading; tag with --cleanup=whitespace");
  return `${notes}\n`;
}
