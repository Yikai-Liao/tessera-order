// SPDX-License-Identifier: BSD-2-Clause
import { execFileSync } from "node:child_process";
import { existsSync, lstatSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** Collect maintained Markdown, excluding platform templates, ignored output and aliases. */
export function documentPaths(root: string): string[] {
  return execFileSync(
    "git",
    ["ls-files", "--cached", "--others", "--exclude-standard", "-z", "--", "*.md"],
    { cwd: root, encoding: "utf8" },
  )
    .split("\0")
    .filter(
      (path) =>
        path &&
        !path.startsWith(".github/") &&
        existsSync(join(root, path)) &&
        !lstatSync(join(root, path)).isSymbolicLink(),
    )
    .sort();
}

/** Build a disposable OKF view; source paths and document bytes remain unchanged. */
export function buildKnowledge(root: string): void {
  const version = JSON.parse(execFileSync("okf", ["version"], { encoding: "utf8" }));
  if (version.version !== "0.5.2")
    throw new Error("Expected okf 0.5.2; see docs/development/knowledge.md");
  const paths = documentPaths(root);
  const bundle = join(root, ".knowledge");
  rmSync(bundle, { recursive: true, force: true });
  for (const path of paths) {
    const target = join(bundle, path);
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, readFileSync(join(root, path)));
  }
  writeFileSync(join(bundle, "index.md"), '---\nokf_version: "0.2"\n---\n');
  const validation = JSON.parse(execFileSync("okf", ["validate", bundle], { encoding: "utf8" }));
  const metadata =
    validation.findings?.filter((finding: { rule: string }) =>
      finding.rule.startsWith("okf/frontmatter/"),
    ) ?? [];
  if (metadata.length) throw new Error(JSON.stringify(metadata, null, 2));
  execFileSync("okf", ["index", bundle], { stdio: "inherit" });
  console.log(JSON.stringify({ documents: paths.length, bundle, warnings: validation.warnings }));
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  buildKnowledge(resolve(dirname(fileURLToPath(import.meta.url)), ".."));
}
