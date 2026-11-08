// SPDX-License-Identifier: BSD-2-Clause

import { execFileSync } from "node:child_process";
import { access, appendFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

/** Select language checks from changed paths; unknown shared files fail open. */
export function select(paths: string[]) {
  const selected = { rust: false, cpp: false, python: false };
  for (const path of paths) {
    if (
      ["docs/.vitepress/", "docs/public/", ".github/scripts/"].some((dir) =>
        path.startsWith(dir),
      ) ||
      [
        "package.json",
        "pnpm-lock.yaml",
        "pnpm-workspace.yaml",
        "tsconfig.json",
        ".github/workflows/docs.yml",
        "biome.json",
      ].includes(path)
    )
      continue;
    if (
      ["rust/", "bench/rust/"].some((dir) => path.startsWith(dir)) ||
      ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "rustfmt.toml"].includes(path)
    ) {
      selected.rust = selected.python = true;
    } else if (["python/", "bench/python/"].some((dir) => path.startsWith(dir))) {
      selected.python = true;
    } else if (
      ["cpp/", "bench/cpp/"].some((dir) => path.startsWith(dir)) ||
      ["CMakeLists.txt", "CMakePresets.json"].includes(path)
    ) {
      selected.cpp = true;
    } else if (
      path.startsWith("assets/") ||
      (path.endsWith(".md") && !path.startsWith("conformance/"))
    )
      continue;
    else selected.rust = selected.cpp = selected.python = true;
  }
  return selected;
}

/** Resolve the branch diff, conservatively selecting all languages if unavailable. */
export function changedPaths(base: string | undefined, head: string): string[] | null {
  if (!base || /^0+$/.test(base)) return null;
  try {
    const start = execFileSync("git", ["merge-base", base, head], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    }).trim();
    return execFileSync("git", ["diff", "--no-renames", "--name-only", "-z", start, head], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    })
      .split("\0")
      .filter(Boolean);
  } catch {
    return null;
  }
}

/** Write Actions outputs while keeping language routing identical for local callers. */
export async function routeChanges(base: string | undefined, head = "HEAD", out?: string) {
  const paths = changedPaths(base, head);
  const present = async (path: string) => {
    try {
      await access(path);
      return true;
    } catch {
      return false;
    }
  };
  const selected = paths === null ? { rust: true, cpp: true, python: true } : select(paths);
  const result = {
    ...selected,
    cpp_present: await present("cpp/CMakeLists.txt"),
    python_present:
      (await present("python/pyproject.toml")) && (await present("python/Cargo.toml")),
  };
  if (out)
    await appendFile(
      out,
      Object.entries(result)
        .map(([key, value]) => `${key}=${value}\n`)
        .join(""),
    );
  return result;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const { values } = parseArgs({
    options: {
      base: { type: "string" },
      head: { type: "string", default: "HEAD" },
      out: { type: "string" },
    },
  });
  console.log(JSON.stringify(await routeChanges(values.base, values.head, values.out)));
}
