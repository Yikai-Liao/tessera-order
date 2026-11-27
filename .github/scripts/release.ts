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
 * needs no manual step. Reject lightweight tags and empty messages before
 * anything is published.
 */
export function readTagNotes(tag: string, cwd?: string): string {
  const git = (...args: string[]) =>
    execFileSync("git", args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
  const ref = `refs/tags/${tag}`;
  if (git("cat-file", "-t", ref).trim() !== "tag")
    throw new Error(`${tag} is not an annotated tag; its message holds the release notes`);
  return tagNotes(git("cat-file", "tag", ref));
}

/** Extract the message of a raw tag object (`git cat-file tag`), without its signature. */
export function tagNotes(tagObject: string): string {
  const headerEnd = tagObject.indexOf("\n\n");
  const message = headerEnd < 0 ? "" : tagObject.slice(headerEnd + 2);
  const signature = message.search(/^-----BEGIN [A-Z ]*SIGNATURE-----$/m);
  const notes = (signature < 0 ? message : message.slice(0, signature)).trim();
  if (notes === "") throw new Error("release tag message is empty; it holds the release notes");
  return `${notes}\n`;
}
