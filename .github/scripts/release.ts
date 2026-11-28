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

export interface VerifyReleaseOptions {
  /** Confirm the embedded Rust crate version is published and not yanked on crates.io. */
  checkPublished?: boolean;
  /** Override sparse-index fetching in tests. */
  fetchIndex?: (url: string) => Promise<string>;
}

const CRATES_INDEX_URL = "https://index.crates.io/te/ss/tessera-order";

async function defaultFetchIndex(url: string): Promise<string> {
  const response = await fetch(url, {
    headers: { "user-agent": "tessera-order-release-verify" },
  });
  if (!response.ok)
    throw new Error(`Failed to fetch crates.io index (${response.status} ${response.statusText})`);
  return response.text();
}

/** Verify Python package and adapter versions against the embedded Rust core. */
export async function verifyPythonRelease(
  tag: string,
  root: string,
  options: VerifyReleaseOptions = {},
) {
  const core = parse(await readFile(join(root, "rust/tessera-order/Cargo.toml"), "utf8"));
  const pyproject = parse(await readFile(join(root, "python/pyproject.toml"), "utf8"));
  const adapter = parse(await readFile(join(root, "python/Cargo.toml"), "utf8"));
  const coreVersion = (core.package as { version: string }).version;
  const version = (pyproject.project as { version: string }).version;
  if (!tag.startsWith("python/v") || tag !== `python/v${version}`)
    throw new Error("Python tag/project version mismatch; expected python/v<project.version>");
  const suffix = version.slice(coreVersion.length);
  if (
    version !== coreVersion &&
    !(version.startsWith(coreVersion) && /^\.post[1-9][0-9]*$/.test(suffix))
  )
    throw new Error(
      `Python project version must be ${coreVersion} or ${coreVersion}.postN (N >= 1, no leading zeros)`,
    );
  const pkg = adapter.package as { version: string; publish?: boolean };
  if (pkg.version !== coreVersion || pkg.publish !== false)
    throw new Error(
      "Python Cargo package.version must match the Rust core and package.publish must be false",
    );
  const deps = adapter.dependencies as Record<string, { path?: string } | string> | undefined;
  const coreDep = deps?.["tessera-order"];
  if (typeof coreDep !== "object" || coreDep?.path !== "../rust/tessera-order")
    throw new Error("Python Cargo dependency tessera-order must point to ../rust/tessera-order");
  const coreTag = `rust/v${coreVersion}`;
  const git = (...args: string[]) =>
    execFileSync("git", args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
  try {
    git("rev-parse", "--verify", "--quiet", `refs/tags/${coreTag}^{commit}`);
  } catch {
    throw new Error(
      `Published Rust release tag ${coreTag} not found; release Rust ${coreVersion} before Python ${version}`,
    );
  }
  const diff = git(
    "diff",
    "--name-only",
    `refs/tags/${coreTag}`,
    "--",
    "rust/tessera-order",
  ).trim();
  const untracked = git(
    "status",
    "--porcelain",
    "--untracked-files=all",
    "--",
    "rust/tessera-order",
  ).trim();
  if (diff !== "" || untracked !== "")
    throw new Error(
      `Embedded rust/tessera-order source differs from published tag ${coreTag}; bump and release Rust before tagging Python`,
    );
  if (options.checkPublished) {
    const fetchIndex = options.fetchIndex ?? defaultFetchIndex;
    const raw = await fetchIndex(CRATES_INDEX_URL);
    const published = raw
      .split("\n")
      .filter((line) => line.trim() !== "")
      .some((line) => {
        const entry = JSON.parse(line) as { vers?: string; yanked?: boolean };
        return entry.vers === coreVersion && entry.yanked !== true;
      });
    if (!published)
      throw new Error(
        `Rust crate tessera-order ${coreVersion} is not published (or is yanked) on crates.io`,
      );
  }
  return { tag, state: "verified" };
}

/** Dispatch release verification by the independently versioned package tag. */
export async function verifyRelease(tag: string, root: string, options: VerifyReleaseOptions = {}) {
  if (tag.startsWith("rust/v")) return verifyRustRelease(tag, root);
  if (tag.startsWith("python/v")) return verifyPythonRelease(tag, root, options);
  throw new Error(`Unsupported release tag ${tag}; expected rust/v<version> or python/v<version>`);
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

// Git's signature markers (`gpg-interface.c`). A signature is the last such
// block and runs to the end of the message; a quoted marker is note text.
const signatureStart =
  /^-----BEGIN (PGP SIGNATURE|PGP MESSAGE|SSH SIGNATURE|SIGNED MESSAGE)-----$/gm;

/** Extract the message of a raw tag object (`git cat-file tag`), without its signature. */
export function tagNotes(tagObject: string): string {
  const headerEnd = tagObject.indexOf("\n\n");
  const message = headerEnd < 0 ? "" : tagObject.slice(headerEnd + 2);
  const last = [...message.matchAll(signatureStart)].at(-1);
  const signed =
    last !== undefined && message.slice(last.index).trimEnd().endsWith(`-----END ${last[1]}-----`);
  // Keep leading indentation: an indented first line is a Markdown code block.
  const notes = (signed ? message.slice(0, last.index) : message).replace(/^\n+/, "").trimEnd();
  if (notes === "") throw new Error("release tag message is empty; it holds the release notes");
  // Git's default `--cleanup=strip` deletes every line that starts with `#`,
  // headings and code alike. Notes grouped under `##` headings always keep
  // one, so none left means the cleanup ran. This is not a Markdown check.
  if (!/^#/m.test(notes))
    throw new Error("release notes have no line starting with #; tag with --cleanup=whitespace");
  return `${notes}\n`;
}
