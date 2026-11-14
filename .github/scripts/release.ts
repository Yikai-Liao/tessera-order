// SPDX-License-Identifier: BSD-2-Clause

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
