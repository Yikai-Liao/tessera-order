// SPDX-License-Identifier: BSD-2-Clause

import { execFileSync } from "node:child_process";
import { access, appendFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

/** Select automatic checks once; known presentation tooling needs no Rust build. */
export function select(paths: string[]) {
  const selected = { rust: false, cpp: false, comparison: false, benchmark: false, docs: false };
  for (const path of paths) {
    if (path.startsWith("docs/") || path.endsWith(".md") || path.startsWith("assets/")) {
      selected.docs = true;
      if (!path.startsWith("conformance/")) continue;
    }
    if (
      path.startsWith("rust/") ||
      ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "rust-toolchain"].includes(path) ||
      path.startsWith(".cargo/")
    ) {
      selected.rust = selected.comparison = selected.benchmark = true;
    } else if (
      path.startsWith("cpp/") ||
      path.startsWith("bench/cpp/") ||
      ["CMakeLists.txt", "CMakePresets.json"].includes(path)
    ) {
      selected.cpp = selected.comparison = true;
    } else if (
      path.endsWith(".test.ts") ||
      (path.startsWith("bench/continuous/") && path !== "bench/continuous/paired.ts") ||
      path.startsWith("bench/report/") ||
      path === "bench/local/report.ts" ||
      path === "bench/local/counters.ts" ||
      path === "bench/local/layout.ts" ||
      path.startsWith("bench/plot/") ||
      path.startsWith(".github/scripts/") ||
      path === "tools/status.ts" ||
      path === ".github/workflows/bench-diagnostics.yml"
    ) {
      if (path.startsWith("bench/plot/")) selected.docs = true;
    } else if (path.startsWith("bench/rust/")) {
      selected.rust = selected.comparison = true;
      if (!path.startsWith("bench/rust/native/")) selected.benchmark = true;
    } else if (path.startsWith("bench/plans/")) {
      if (path === "bench/plans/continuous.toml") selected.benchmark = true;
      else if (path !== "bench/plans/diagnostics.toml") selected.comparison = true;
    } else if (path.startsWith("bench/") || path.startsWith("tools/")) {
      selected.comparison = selected.benchmark = true;
      if (path.startsWith("bench/catalog.")) selected.docs = true;
    } else if (
      [
        "package.json",
        "pnpm-lock.yaml",
        "pnpm-workspace.yaml",
        "tsconfig.json",
        "biome.json",
      ].includes(path)
    ) {
      selected.comparison = selected.benchmark = selected.docs = true;
    } else if (path === ".github/workflows/docs.yml") {
      selected.docs = true;
    } else if (
      path === "rustfmt.toml" ||
      path === ".github/workflows/rust-ci.yml" ||
      path.startsWith(".github/actions/rust-test/")
    ) {
      selected.rust = true;
    } else {
      Object.assign(selected, {
        rust: true,
        cpp: true,
        comparison: true,
        benchmark: true,
        docs: true,
      });
    }
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
  const selected = paths === null ? select(["unknown"]) : select(paths);
  const result = {
    ...selected,
    cpp_present: await present("cpp/CMakeLists.txt"),
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
