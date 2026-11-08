// SPDX-License-Identifier: BSD-2-Clause

import { createHash } from "node:crypto";
import { createWriteStream } from "node:fs";
import { mkdir, mkdtemp, readdir, readFile, rename, rm, stat, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { Transform } from "node:stream";
import { pipeline } from "node:stream/promises";
import { x as extract } from "tar";
import { type NativeSource, nativeSources, treeHash } from "./sources.ts";

/** Fetch, hash-check and install one pinned source on the destination filesystem. */
export async function fetchNativeSource(vendor: string, name: string, source: NativeSource) {
  const target = join(vendor, name);
  const stamp = join(target, ".tessera-source.json");
  try {
    const state = JSON.parse(await readFile(stamp, "utf8"));
    if (
      Object.entries(source).every(
        ([key, value]) => JSON.stringify(state[key]) === JSON.stringify(value),
      ) &&
      state.tree_sha256 === (await treeHash(target))
    )
      return "verified";
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
  }
  if (
    !/^https:\/\/github\.com\/[\w.-]+\/[\w.-]+$/.test(source.repository) ||
    !/^[a-f0-9]{40}$/.test(source.commit)
  )
    throw new Error(`${name}: expected a pinned GitHub source`);
  if (
    source.paths &&
    (!source.paths.length ||
      source.paths.some(
        (path) => !path || path.split("/").some((part) => !part || part === "." || part === ".."),
      ))
  )
    throw new Error(`${name}: expected nonempty archive-relative source paths`);
  await mkdir(vendor, { recursive: true });
  // Staging beside the destination keeps installation on one filesystem.
  const directory = await mkdtemp(join(vendor, ".source-"));
  try {
    const archive = join(directory, "source.tar.gz");
    const response = await fetch(
      `https://codeload.github.com/${source.repository.slice("https://github.com/".length)}/tar.gz/${source.commit}`,
      { signal: AbortSignal.timeout(120_000) },
    );
    if (!response.ok || !response.body)
      throw new Error(`${name}: download failed (${response.status})`);
    const digest = createHash("sha256");
    const hash = new Transform({
      transform(chunk, _encoding, callback) {
        digest.update(chunk);
        callback(null, chunk);
      },
    });
    await pipeline(response.body, hash, createWriteStream(archive, { flags: "wx" }));
    if (digest.digest("hex") !== source.sha256)
      throw new Error(`${name}: source archive SHA-256 mismatch`);
    const unpacked = join(directory, "unpacked");
    await mkdir(unpacked);
    await extract({
      file: archive,
      cwd: unpacked,
      strict: true,
      filter(path) {
        if (!source.paths) return true;
        const relative = path.split("/").slice(1).join("/").replace(/\/$/, "");
        return (
          !relative ||
          source.paths.some(
            (selected) =>
              relative === selected ||
              relative.startsWith(`${selected}/`) ||
              selected.startsWith(`${relative}/`),
          )
        );
      },
    });
    const entries = await readdir(unpacked);
    if (entries.length !== 1) throw new Error(`${name}: expected one archive root`);
    const tree = join(unpacked, entries[0]);
    if (!(await stat(tree)).isDirectory())
      throw new Error(`${name}: archive root is not a directory`);
    for (const license of source.license_files)
      if (!(await stat(join(tree, license))).isFile())
        throw new Error(`${name}: missing license ${license}`);
    await writeFile(
      join(tree, ".tessera-source.json"),
      `${JSON.stringify({ ...source, tree_sha256: await treeHash(tree) }, null, 2)}\n`,
    );
    await rm(target, { recursive: true, force: true });
    await rename(tree, target);
    return "fetched";
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

/** Fetch pinned archives separately from offline compilation. */
export async function fetchNative(root: string) {
  const sources = await nativeSources();
  const vendor = join(root, "bench/vendor");
  const results: Record<string, string> = {};
  for (const [name, source] of Object.entries(sources))
    results[name] = await fetchNativeSource(vendor, name, source);
  return results;
}
