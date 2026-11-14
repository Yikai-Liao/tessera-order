// SPDX-License-Identifier: BSD-2-Clause

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
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
    /^\.github\/workflows\/(dco|pr-title|release|rust-release|cpp-release|bench-diagnostics|main-benchmarks)\.yml$/,
    [],
  ],
  // Tests exercise the engine but are not the code that PR measurements compare.
  [
    /^rust\/[^/]+\/(tests\/|LICENSE$)|^(rustfmt\.toml|\.github\/workflows\/rust-ci\.yml|\.github\/actions\/rust-test\/)/,
    ["rust"],
  ],
  // The figures replay the library through this WebAssembly crate.
  [/^rust\/tessera-trace\//, ["docs"]],
  [/^rust\/tessera-order\/(src\/|Cargo\.toml$)/, ["rust", "benchmark", "docs"]],
  [/^rust\//, ["rust", "benchmark"]],
  [
    /^\.cargo\/|^(Cargo\.toml|Cargo\.lock|rust-toolchain(\.toml)?)$/,
    ["rust", "comparison", "benchmark"],
  ],
  [
    /^(cpp\/|bench\/cpp\/)|^(CMakeLists\.txt|CMakePresets\.json|\.github\/workflows\/cpp-ci\.yml)$/,
    ["cpp"],
  ],
  [/^\.github\/scripts\/install_native_tools\.sh$/, ["comparison"]],
  // Tooling that renders or reads results and never builds or times a sorting algorithm.
  [/^bench\/plot\//, ["docs"]],
  [/^bench\/tracking\/(result|history)\.ts$/, ["benchmark", "docs"]],
  [
    /\.test\.ts$|^bench\/(code-(change|source)\.ts$|continuous\/(?!paired\.ts$)|report\/|local\/(report|counters|layout)\.ts$)|^\.github\/scripts\/|^tools\/(code|status|repository)\.ts$|^(biome|tsconfig)\.json$/,
    [],
  ],
  [
    /^bench\/tracking\/|^bench\/continuous\/paired\.ts$|^bench\/local\/(runner|model|samples)\.ts$|^tools\/continuous\.ts$/,
    ["benchmark"],
  ],
  [/^bench\/(session|revisions)\.ts$/, ["comparison"]],
  [
    /^bench\/rust\/(native\/|tests\/|src\/(allocation\.rs$|bin\/(compare|memory)\.rs$))/,
    ["comparison"],
  ],
  [/^bench\/rust\//, ["comparison", "benchmark"]],
  [/^bench\/plans\/diagnostics\.toml$/, []],
  [/^bench\/plans\//, ["comparison", "benchmark"]],
  [/^bench\/catalog\./, ["comparison", "benchmark", "docs"]],
  [/^(bench|tools)\//, ["comparison", "benchmark"]],
  [/^(package\.json|bun\.lock)$/, ["comparison", "benchmark", "docs"]],
  [/^\.github\/workflows\/docs\.yml$/, ["docs"]],
];

/**
 * Files that the router, the marker upload or Bun read while deciding reuse. A
 * PR that changes one in any commit reuses nothing, because one of its runs may
 * have recorded markers its own altered code invented.
 */
const TRUSTED = /^(\.github\/|\.env|package\.json$|bunfig\.toml$|tsconfig\.json$)/;

const lanesOf = (path: string) => ROUTES.find(([pattern]) => pattern.test(path))?.[1] ?? LANES;

/** Select automatic checks once; paths no row names select every lane. */
export function select(paths: string[]) {
  const selected = { rust: false, cpp: false, comparison: false, benchmark: false, docs: false };
  for (const path of paths) for (const lane of lanesOf(path)) selected[lane] = true;
  return selected;
}

const git = (...args: string[]) =>
  execFileSync("git", args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });

/** Resolve the branch diff, conservatively selecting all languages if unavailable. */
export function changedPaths(base: string | undefined, head: string): string[] | null {
  if (!base || /^0+$/.test(base)) return null;
  try {
    const start = git("merge-base", base, head).trim();
    return git("diff", "--no-renames", "--name-only", "-z", start, head)
      .split("\0")
      .filter(Boolean);
  } catch {
    return null;
  }
}

/** List the tracked entries, with mode and blob ID, whose route includes the lane. */
function inputs(revision: string, lane: Lane) {
  return git("ls-tree", "-r", "-z", revision)
    .split("\0")
    .filter((entry) => entry && lanesOf(entry.slice(entry.indexOf("\t") + 1)).includes(lane))
    .join("\0");
}

/**
 * Name the marker of a lane's verified inputs. Checks run on the tested tree;
 * the measurement compares the PR head with its merge base instead.
 */
export function marker(lane: Lane, revisions: { tested: string; head: string; baseline: string }) {
  const text =
    lane === "benchmark"
      ? `${inputs(revisions.head, lane)}\0\0${inputs(revisions.baseline, lane)}`
      : inputs(revisions.tested, lane);
  return `ci-pass-${lane}-${createHash("sha256").update(text).digest("hex").slice(0, 40)}`;
}

const isAncestor = (commit: string, descendant: string) => {
  try {
    git("merge-base", "--is-ancestor", commit, descendant);
    return true;
  } catch {
    return false;
  }
};

/** Whether any commit after the merge base touched a file the reuse decision trusts. */
export function changesTrustedFiles(baseline: string, head: string) {
  return git("log", "-m", "--name-only", "-z", "--format=", `${baseline}..${head}`)
    .split("\0")
    .some((path) => TRUSTED.test(path.trim()));
}

interface Artifact {
  expired: boolean;
  workflow_run: { id: number; head_sha: string };
}

/**
 * Skip selected lanes that an earlier run of this PR passed with identical
 * inputs. A marker is an artifact, so it names the run that uploaded it; only
 * runs on this PR's own commits count, and only while none of them changed a
 * trusted file. Any lookup failure runs the lane.
 */
async function reuse(
  selected: ReturnType<typeof select>,
  revisions: { base: string; head: string; tested: string },
  github: { repository: string; server: string; token: string },
) {
  const baseline = git("merge-base", revisions.base, revisions.head).trim();
  const markers = LANES.filter((lane) => selected[lane]).map((lane) => ({
    lane,
    name: marker(lane, { ...revisions, baseline }),
  }));
  const reused: Partial<Record<Lane, Artifact["workflow_run"]>> = {};
  if (!changesTrustedFiles(baseline, revisions.head))
    for (const { lane, name } of markers) {
      const response = await fetch(
        `https://api.github.com/repos/${github.repository}/actions/artifacts?name=${name}`,
        {
          headers: {
            authorization: `Bearer ${github.token}`,
            accept: "application/vnd.github+json",
          },
        },
      );
      if (!response.ok) continue;
      const { artifacts } = (await response.json()) as { artifacts: Artifact[] };
      reused[lane] = artifacts.find(
        ({ expired, workflow_run: { head_sha } }) =>
          !expired && isAncestor(head_sha, revisions.head) && !isAncestor(head_sha, baseline),
      )?.workflow_run;
    }
  const summary = Object.entries(reused)
    .map(
      ([lane, run]) =>
        `- ${lane}: reused [run ${run.id}](${github.server}/${github.repository}/actions/runs/${run.id}) for identical inputs\n`,
    )
    .join("");
  return {
    lanes: Object.fromEntries(LANES.map((lane) => [lane, selected[lane] && !reused[lane]])),
    record: JSON.stringify(markers.map(({ name }) => name)),
    benchmark_reused: reused.benchmark?.head_sha ?? "",
    summary,
  };
}

/**
 * Write Actions outputs while keeping language routing identical for local callers.
 * With `reuse`, a PR run also skips lanes verified earlier and names the markers
 * that `ci-ok` records.
 */
export async function routeChanges(
  base: string | undefined,
  head = "HEAD",
  out?: string,
  reuseFrom?: { tested: string; repository: string; server: string; token: string },
) {
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
  const reused =
    reuseFrom && base && paths !== null
      ? await reuse(selected, { base, head, tested: reuseFrom.tested }, reuseFrom)
      : null;
  const result = {
    ...selected,
    ...reused?.lanes,
    cpp_present: await present("cpp/CMakeLists.txt"),
    record: reused?.record ?? "[]",
    benchmark_reused: reused?.benchmark_reused ?? "",
  };
  if (out)
    await appendFile(
      out,
      Object.entries(result)
        .map(([key, value]) => `${key}=${value}\n`)
        .join(""),
    );
  if (reused?.summary && process.env.GITHUB_STEP_SUMMARY)
    await appendFile(process.env.GITHUB_STEP_SUMMARY, reused.summary);
  return result;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const { values } = parseArgs({
    options: {
      base: { type: "string" },
      head: { type: "string", default: "HEAD" },
      out: { type: "string" },
      // The tested tree of a PR run, normally its merge commit; enables reuse.
      reuse: { type: "string" },
    },
  });
  const github = {
    repository: process.env.GITHUB_REPOSITORY ?? "",
    server: process.env.GITHUB_SERVER_URL ?? "https://github.com",
    token: process.env.GITHUB_TOKEN ?? "",
  };
  const reuseFrom = values.reuse ? { tested: values.reuse, ...github } : undefined;
  console.log(JSON.stringify(await routeChanges(values.base, values.head, values.out, reuseFrom)));
}
