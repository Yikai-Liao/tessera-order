// SPDX-License-Identifier: BSD-2-Clause

import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { parse, stringify } from "smol-toml";
import { command } from "../tools/process.ts";
import {
  copyLibraryRevision,
  copySnapshot,
  resolveCommit,
  simdFeatures,
  sourceHash,
} from "./workspace.ts";

/** Revisions linked into one executable; omitted current means the working snapshot. */
export interface Revisions {
  baseline: string;
  current?: string;
}

/** Materialize named library copies without changing their Rust sources. */
export async function prepareRevisions(
  root: string,
  tree: string,
  revisions: Revisions,
  features = ["compare"],
) {
  const baseline = await resolveCommit(root, revisions.baseline);
  const current = revisions.current ? await resolveCommit(root, revisions.current) : undefined;
  // Fresh CI hosts need the locked registry metadata before offline path resolution.
  await command("cargo", ["fetch", "--locked"], { cwd: tree });
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
  const sources: Record<string, string> = {};
  const primary = dependencies["tessera-sort"] as { features?: string[] };
  primary.features = [
    ...new Set([
      ...(primary.features ?? []),
      ...simdFeatures(await readFile(join(library, "Cargo.toml"), "utf8")),
      ...(features.includes("rayon") ? ["rayon"] : []),
    ]),
  ];
  for (const name of ["revision-baseline", "revision-baseline-copy", "revision-current-copy"]) {
    const destination = join(copies, name);
    if (name === "revision-current-copy") await copySnapshot(library, destination, tree);
    else await copyLibraryRevision(root, baseline, destination);
    sources[name] = await sourceHash(destination);
    const path = join(destination, "Cargo.toml");
    const copy = parse(await readFile(path, "utf8"));
    (copy.package as { name: string }).name = name;
    const text = stringify(copy);
    await writeFile(path, text);
    dependencies[name] = {
      path: `../revision-libraries/${name}`,
      features: [...simdFeatures(text), ...(features.includes("rayon") ? ["rayon"] : [])],
    };
  }
  await writeFile(manifestPath, stringify(manifest));
  const provenance = {
    schema: 1,
    baseline,
    current: current ?? (await resolveCommit(root, "HEAD")),
    workingTree: current === undefined,
    // The current copy was hashed from this library before its manifest rename.
    currentSource: sources["revision-current-copy"],
    copies: sources,
  };
  const originalLock = await readFile(join(tree, "Cargo.lock"), "utf8");
  // Resolve only the added path packages against the existing registry lock entries.
  const metadata = JSON.parse(
    await command(
      "cargo",
      [
        "metadata",
        "--offline",
        "--format-version=1",
        "--features",
        features.map((feature) => `tessera-bench/${feature}`).join(","),
      ],
      {
        cwd: tree,
        capture: true,
      },
    ),
  ) as {
    packages: { name: string; id: string }[];
    resolve: { nodes: { id: string; features: string[] }[] };
  };
  verifyRevisionLock(originalLock, await readFile(join(tree, "Cargo.lock"), "utf8"));
  const libraries = Object.fromEntries(
    ["tessera-sort", ...Object.keys(sources)].map((name) => {
      const id = metadata.packages.find((entry) => entry.name === name)!.id;
      return [name, metadata.resolve.nodes.find((entry) => entry.id === id)!.features.sort()];
    }),
  );
  await writeFile(
    join(tree, "bench/rust/revisions.json"),
    JSON.stringify({ ...provenance, libraries }, null, 2),
  );
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
