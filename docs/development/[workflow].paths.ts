// SPDX-License-Identifier: BSD-2-Clause
import { readFileSync } from "node:fs";
import { basename, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

/** Canonical task skills and their human-readable site pages. */
export const workflowPages = new Map([
  [".agents/skills/tessera-engine/SKILL.md", "development/change-engine.md"],
  [".agents/skills/tessera-measure/SKILL.md", "development/measure-change.md"],
  [".agents/skills/tessera-pr-performance/SKILL.md", "development/pr-performance.md"],
]);

/** Recover a workflow source when rendering its virtual page or edit link. */
export function workflowSource(page: string): string | undefined {
  const served = page.replace(/^docs\//, "");
  return [...workflowPages].find(([, target]) => target === served)?.[0];
}

export default {
  paths() {
    return [...workflowPages].map(([source, page]) => ({
      params: { workflow: basename(page, ".md") },
      content: readFileSync(resolve(repoRoot, source), "utf8"),
    }));
  },
};
