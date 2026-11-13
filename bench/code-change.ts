// SPDX-License-Identifier: BSD-2-Clause

import { lstatSync } from "node:fs";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { diffLines } from "diff";
import { command } from "../tools/process.ts";
import { copyLibraryRevision, copySnapshot, resolveCommit } from "./workspace.ts";

export interface CodeChange {
  baseHash: string;
  hash: string | null;
  baseline: number;
  current: number;
  added: number;
  removed: number;
  files: { path: string; baseline: number; current: number; added: number; removed: number }[];
}

const SOURCE = "rust/tessera-sort/src/";
const ENGINE =
  /^rust\/tessera-sort\/src\/(engine|block|key|portable_simd|scan|unwind)(\.rs$|\/.*\.rs$)/;

/** Compare core production lines from commits or the working tree; owns temporary snapshots. */
export async function codeChange(
  baseRef: string,
  currentRef?: string,
  root = process.cwd(),
): Promise<CodeChange> {
  // Keep native parsing out of unrelated CLI commands.
  const { production } = await import("./code-source.ts");
  const git = (args: string[]) => command("git", args, { cwd: root, capture: true });
  const baseHash = await resolveCommit(root, baseRef);
  const hash = currentRef === undefined ? null : await resolveCommit(root, currentRef);
  const temporary = await mkdtemp(join(tmpdir(), "tessera-code-"));
  async function snapshot(revision: string | null) {
    const paths = (
      await git(
        revision
          ? ["ls-tree", "-r", "--name-only", "-z", revision, "--", SOURCE]
          : ["ls-files", "--cached", "--others", "--exclude-standard", "-z", "--", SOURCE],
      )
    )
      .split("\0")
      .filter((path) => ENGINE.test(path));
    const files = new Map<string, string>();
    if (!paths.length) return files;
    const tree = join(temporary, revision ?? "working");
    if (revision) await copyLibraryRevision(root, revision, tree);
    else if (lstatSync(join(root, "rust/tessera-sort"), { throwIfNoEntry: false }))
      await copySnapshot(join(root, "rust/tessera-sort"), tree, root);
    else return files;
    const excluded: string[] = [];
    for (const path of new Set(paths)) {
      const source = await readFile(
        join(tree, path.slice("rust/tessera-sort/".length)),
        "utf8",
      ).catch((error: NodeJS.ErrnoException) => {
        if (!revision && error.code === "ENOENT") return "";
        throw error;
      });
      const { lines, testModules } = production(source);
      files.set(path.slice(SOURCE.length), lines.length ? `${lines.join("\n")}\n` : "");
      const directory = path.endsWith("/mod.rs") ? dirname(path) : path.slice(0, -3);
      for (const name of testModules) excluded.push(`${directory}/${name}`.slice(SOURCE.length));
    }
    for (const path of files.keys())
      if (excluded.some((prefix) => path === `${prefix}.rs` || path.startsWith(`${prefix}/`)))
        files.delete(path);
    return files;
  }
  try {
    const before = await snapshot(baseHash);
    const after = await snapshot(hash);
    const files = [...new Set([...before.keys(), ...after.keys()])]
      .map((path) => {
        const changes = diffLines(before.get(path) ?? "", after.get(path) ?? "");
        const count = (side: "added" | "removed") =>
          changes.filter((part) => part[side]).reduce((sum, part) => sum + part.count, 0);
        return {
          path,
          baseline: (before.get(path)?.match(/\n/g) ?? []).length,
          current: (after.get(path)?.match(/\n/g) ?? []).length,
          added: count("added"),
          removed: count("removed"),
        };
      })
      .sort((a, b) => b.added + b.removed - a.added - a.removed || a.path.localeCompare(b.path));
    const total = (key: "baseline" | "current" | "added" | "removed") =>
      files.reduce((sum, file) => sum + file[key], 0);
    return {
      baseHash,
      hash,
      baseline: total("baseline"),
      current: total("current"),
      added: total("added"),
      removed: total("removed"),
      files,
    };
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

/** Show totals and changed files; detailed local output also includes unchanged files. */
export function codeTable(
  change: CodeChange,
  options: { baseline: string; current: string; details?: boolean },
) {
  const signed = (value: number) => `${value > 0 ? "+" : value < 0 ? "−" : ""}${Math.abs(value)}`;
  const cell = (text: string) => text.replaceAll("|", "\\|").replace(/[\r\n]/g, " ");
  const row = (
    label: string,
    value: Pick<CodeChange, "baseline" | "current" | "added" | "removed">,
  ) =>
    `| ${label} | ${value.baseline} | ${value.current} | ${value.added} | ${value.removed} | ${signed(value.current - value.baseline)} |`;
  return [
    `| Production LOC | ${cell(options.baseline)} | ${cell(options.current)} | Added | Removed | Net |`,
    "|---|---:|---:|---:|---:|---:|",
    row("Core engine", change),
    ...change.files
      .filter((file) => options.details || file.added || file.removed)
      .map((file) => row(`\`${cell(file.path)}\``, file)),
  ].join("\n");
}
