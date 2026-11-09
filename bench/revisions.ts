// SPDX-License-Identifier: BSD-2-Clause

import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { parse, stringify } from "smol-toml";
import { command } from "../tools/process.ts";
import { copyLibraryRevision, copySnapshot, sourceHash } from "./workspace.ts";

/** Revisions linked into one executable; omitted current means the working snapshot. */
export interface Revisions {
  baseline: string;
  current?: string;
}

/** Materialize named library copies without changing their Rust sources. */
export async function prepareRevisions(root: string, tree: string, revisions: Revisions) {
  const commit = (ref: string) =>
    command("git", ["rev-parse", "--verify", "--end-of-options", `${ref}^{commit}`], {
      cwd: root,
      capture: true,
    });
  const baseline = await commit(revisions.baseline);
  const current = revisions.current ? await commit(revisions.current) : undefined;
  const library = join(tree, "rust/tessera-sort");
  if (current) {
    // The current harness stays in place; only the requested library is replaced.
    await rm(library, { recursive: true });
    await copyLibraryRevision(root, current, library);
  }
  const copies = join(tree, "bench/revision-libraries");
  await mkdir(copies);
  const manifestPath = join(tree, "bench/rust/Cargo.toml");
  const manifest = parse(await readFile(manifestPath, "utf8"));
  const dependencies = manifest.dependencies as Record<string, object>;
  const features = manifest.features as Record<string, string[]>;
  const sources: Record<string, string> = {};
  for (const name of ["revision-baseline", "revision-baseline-copy", "revision-current-copy"]) {
    const destination = join(copies, name);
    if (name === "revision-current-copy") await copySnapshot(library, destination, tree);
    else await copyLibraryRevision(root, baseline, destination);
    sources[name] = await sourceHash(destination);
    const path = join(destination, "Cargo.toml");
    const copy = parse(await readFile(path, "utf8"));
    (copy.package as { name: string }).name = name;
    await writeFile(path, stringify(copy));
    dependencies[name] = { path: `../revision-libraries/${name}` };
    features["portable-simd"].push(`${name}/portable-simd`);
  }
  await writeFile(manifestPath, stringify(manifest));
  const provenance = {
    schema: 1,
    baseline,
    current: current ?? (await commit("HEAD")),
    workingTree: current === undefined,
    currentSource: await sourceHash(library),
    copies: sources,
  };
  await writeFile(join(tree, "bench/rust/revisions.json"), JSON.stringify(provenance, null, 2));
  const originalLock = await readFile(join(tree, "Cargo.lock"), "utf8");
  // Resolve only the added path packages against the existing registry lock entries.
  await command("cargo", ["metadata", "--offline", "--format-version=1"], {
    cwd: tree,
    capture: true,
  });
  verifyRevisionLock(originalLock, await readFile(join(tree, "Cargo.lock"), "utf8"));
}

/** Reject registry or Git dependency changes while adding local revision packages. */
export function verifyRevisionLock(original: string, resolved: string) {
  const external = (text: string) =>
    ((parse(text).package ?? []) as { source?: string }[])
      .filter((entry) => entry.source !== undefined)
      .map((entry) => JSON.stringify(entry))
      .sort();
  if (JSON.stringify(external(original)) !== JSON.stringify(external(resolved)))
    throw new Error(
      "Revision dependencies differ from the current lockfile; no measurements started",
    );
}
