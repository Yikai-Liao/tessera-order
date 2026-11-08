// SPDX-License-Identifier: BSD-2-Clause

import { createHash } from "node:crypto";
import { copyFile, glob, mkdir, readFile } from "node:fs/promises";
import { basename, join } from "node:path";
import { parse } from "smol-toml";

/** Verify release identities without publishing or requiring a registry credential. */
export async function verifyRelease(language: string, tag: string, root: string) {
  const toml = async (path: string) => parse(await readFile(join(root, path), "utf8"));
  if (language === "rust") {
    const manifest = await toml("rust/tessera-sort/Cargo.toml");
    const pkg = manifest.package as { version: string; publish?: boolean };
    if (tag !== `rust/v${pkg.version}` || pkg.publish === false)
      throw new Error("Rust tag/package version mismatch or package is unpublished");
  } else if (language === "python") {
    const manifest = await toml("python/Cargo.toml");
    const pkg = manifest.package as { version: string };
    const project = (await toml("python/pyproject.toml")).project as { version?: string };
    if ((project.version ?? pkg.version) !== pkg.version || tag !== `python/v${pkg.version}`)
      throw new Error("Python tag/project/extension version mismatch");
  } else throw new Error("Release validation supports rust or python");
  return { language, tag, state: "verified" };
}

/** Collect wheels without overwriting a same-named artifact with different bytes. */
export async function collectWheels(source: string, out: string) {
  await mkdir(out, { recursive: false });
  let count = 0;
  const hashes = new Map<string, string>();
  for await (const path of glob("**/*.whl", { cwd: source })) {
    const name = basename(path);
    const wheel = join(source, path);
    const hash = createHash("sha256")
      .update(await readFile(wheel))
      .digest("hex");
    const previous = hashes.get(name);
    if (previous && previous !== hash) throw new Error(`Conflicting wheel: ${name}`);
    if (!previous) {
      await copyFile(wheel, join(out, name));
      hashes.set(name, hash);
      count++;
    }
  }
  if (!count) throw new Error("No wheels downloaded");
  return { out, wheels: count };
}
