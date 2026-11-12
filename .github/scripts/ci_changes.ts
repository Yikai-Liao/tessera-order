// SPDX-License-Identifier: BSD-2-Clause

import { execFileSync } from "node:child_process";
import { access, appendFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

const LANES = ["rust", "cpp", "comparison", "benchmark", "docs"] as const;
type Lane = (typeof LANES)[number];

/**
 * One row per owner of a path: the lanes whose result that path can change. The
 * first matching row decides, so specific rows precede their directory. A path
 * that no row names selects every lane, and so does an unavailable base, because
 * an unclassified file is not known to be harmless.
 */
const ROUTES: [RegExp, Lane[]][] = [
  // Repository metadata that no automatic lane reads. `fuzz/` is outside the Cargo workspace.
  [/^(\.github\/(ISSUE_TEMPLATE\/|pull_request_template\.md$|dependabot\.yml$)|fuzz\/)/, []],
  // Prose, including the shared contract; a lane needs a row here once an executable consumer exists.
  [/^(docs\/|assets\/)|\.md$/, ["docs"]],
  // These workflows run independently of the automatic build/test pipeline.
  [
    /^\.github\/workflows\/(dco|pr-title|release|rust-release|cpp-release|bench-diagnostics)\.yml$/,
    [],
  ],
  // Tests exercise the engine but are not the code that PR measurements compare.
  [
    /^rust\/[^/]+\/(tests\/|LICENSE$)|^(rustfmt\.toml|\.github\/workflows\/rust-ci\.yml|\.github\/actions\/rust-test\/)/,
    ["rust"],
  ],
  [/^rust\//, ["rust", "benchmark"]],
  [
    /^\.cargo\/|^(Cargo\.toml|Cargo\.lock|rust-toolchain(\.toml)?)$/,
    ["rust", "comparison", "benchmark"],
  ],
  [
    /^(cpp\/|bench\/cpp\/)|^(CMakeLists\.txt|CMakePresets\.json|\.github\/workflows\/cpp-ci\.yml)$/,
    ["cpp"],
  ],
  // Tooling that renders or reads results and never builds or times a sorting algorithm.
  [/^bench\/plot\//, ["docs"]],
  [
    /\.test\.ts$|^bench\/(continuous\/(?!paired\.ts$)|report\/|local\/(report|counters|layout)\.ts$)|^\.github\/scripts\/|^tools\/(status|repository)\.ts$|^(biome|tsconfig)\.json$/,
    [],
  ],
  [
    /^bench\/continuous\/paired\.ts$|^bench\/local\/(runner|model|samples)\.ts$|^tools\/continuous\.ts$/,
    ["benchmark"],
  ],
  [/^bench\/(release|session|revisions)\.ts$/, ["comparison"]],
  [
    /^bench\/rust\/(native\/|tests\/|src\/(comparison\/|allocation\.rs$|bin\/(compare|memory)\.rs$))/,
    ["comparison"],
  ],
  [/^bench\/rust\//, ["comparison", "benchmark"]],
  [/^bench\/plans\/(continuous|local)\.toml$/, ["benchmark"]],
  [/^bench\/plans\/diagnostics\.toml$/, []],
  [/^bench\/plans\//, ["comparison"]],
  [/^bench\/catalog\./, ["comparison", "benchmark", "docs"]],
  [/^(bench|tools)\//, ["comparison", "benchmark"]],
  [/^(package\.json|bun\.lock)$/, ["comparison", "benchmark", "docs"]],
  [/^\.github\/workflows\/docs\.yml$/, ["docs"]],
];

/** Select automatic checks once; paths no row names select every lane. */
export function select(paths: string[]) {
  const selected = { rust: false, cpp: false, comparison: false, benchmark: false, docs: false };
  for (const path of paths)
    for (const lane of ROUTES.find(([pattern]) => pattern.test(path))?.[1] ?? LANES)
      selected[lane] = true;
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
