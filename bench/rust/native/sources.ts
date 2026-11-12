// SPDX-License-Identifier: BSD-2-Clause

import { createHash } from "node:crypto";
import { readdir, readFile, realpath, stat } from "node:fs/promises";
import { isAbsolute, relative, resolve, sep } from "node:path";
import { parse } from "smol-toml";

export interface NativeSource {
  repository: string;
  commit: string;
  sha256: string;
  license: string;
  license_files: string[];
  paths?: string[];
}

/** Read the one pinned native-source manifest shared by fetching and compilation. */
export async function nativeSources() {
  return (
    parse(await readFile(new URL("./sources.toml", import.meta.url), "utf8")) as unknown as {
      sources: Record<string, NativeSource>;
    }
  ).sources;
}

/** Hash POSIX paths in component order, preserving existing Python source stamps. */
export async function treeHash(root: string) {
  const digest = createHash("sha256");
  const boundary = await realpath(root);
  async function visit(directory: string) {
    // Sorted components match pathlib ordering; sorting whole paths does not.
    const entries = (await readdir(directory, { withFileTypes: true })).sort((a, b) =>
      Buffer.compare(Buffer.from(a.name), Buffer.from(b.name)),
    );
    for (const entry of entries) {
      const path = resolve(directory, entry.name);
      if (entry.isDirectory()) await visit(path);
      else if (entry.name !== ".tessera-source.json") {
        const target = await realpath(path);
        const name = relative(boundary, target);
        if (isAbsolute(name) || name.split(sep)[0] === "..")
          throw new Error(`Native source link escapes its tree: ${path}`);
        if ((await stat(path)).isFile())
          digest
            .update(relative(root, path).split(sep).join("/"))
            .update("\0")
            .update(
              createHash("sha256")
                .update(await readFile(path))
                .digest(),
            );
      }
    }
  }
  await visit(root);
  return digest.digest("hex");
}

/** Verify pinned metadata and the extracted tree without contacting the network. */
export async function verifySources(vendor: string, sources: Record<string, NativeSource>) {
  for (const [name, source] of Object.entries(sources)) {
    const path = resolve(vendor, name);
    const stamp = JSON.parse(await readFile(resolve(path, ".tessera-source.json"), "utf8"));
    if (
      Object.entries(source).some(
        ([key, value]) => JSON.stringify(stamp[key]) !== JSON.stringify(value),
      ) ||
      stamp.tree_sha256 !== (await treeHash(path))
    )
      throw new Error(`${name}: source tree differs; run bun run tools native fetch`);
  }
}
