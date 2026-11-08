// SPDX-License-Identifier: BSD-2-Clause
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { buildKnowledge, documentPaths } from "./knowledge.ts";

const concept =
  "---\ntype: Reference\ntitle: Engine safety\ndescription: Check panic recovery.\ntags: [engine, safety]\n---\n\n# Safety\n\nPreserve records when a key panics.\n";

function fixture(): string {
  const root = mkdtempSync(join(tmpdir(), "tessera-knowledge-"));
  execFileSync("git", ["init", "-q", root]);
  writeFileSync(join(root, ".gitignore"), ".knowledge/\nnode_modules/\n");
  mkdirSync(join(root, "docs"));
  writeFileSync(join(root, "docs/engine.md"), concept);
  execFileSync("git", ["add", "."], { cwd: root });
  return root;
}

test("selection excludes generated aliases", () => {
  const root = fixture();
  try {
    writeFileSync(join(root, "new.md"), concept);
    mkdirSync(join(root, ".github"));
    writeFileSync(join(root, ".github/pull_request_template.md"), "## Summary\n");
    symlinkSync("docs/engine.md", join(root, "CLAUDE.md"));
    mkdirSync(join(root, "node_modules"));
    writeFileSync(join(root, "node_modules/README.md"), "vendor");
    writeFileSync(join(root, "deleted.md"), concept);
    execFileSync("git", ["add", "deleted.md"], { cwd: root });
    rmSync(join(root, "deleted.md"));
    assert.deepEqual(documentPaths(root), ["docs/engine.md", "new.md"]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("bundle supports progressive retrieval", () => {
  const root = fixture();
  try {
    writeFileSync(join(root, "docs/other-tag.md"), concept.replace("[engine, safety]", "[docs]"));
    writeFileSync(join(root, "docs/other-type.md"), concept.replace("Reference", "Explanation"));
    writeFileSync(join(root, "docs/other-text.md"), concept.replaceAll("panic", "unwind"));
    buildKnowledge(root);
    const bundle = join(root, ".knowledge");
    assert.equal(readFileSync(join(bundle, "docs/engine.md"), "utf8"), concept);
    assert.match(readFileSync(join(bundle, "index.md"), "utf8"), /okf_version: "0.2"/);
    assert.match(readFileSync(join(bundle, "docs/index.md"), "utf8"), /Check panic recovery/);
    const search = JSON.parse(
      execFileSync(
        "okf",
        ["search", bundle, "--tag", "safety", "--type", "Reference", "--text", "panic"],
        { encoding: "utf8" },
      ),
    );
    assert.deepEqual(
      search.results.map((entry: { id: string }) => entry.id),
      ["docs/engine"],
    );
    assert.ok(search.results.every((entry: object) => !("body" in entry)));
    const shown = JSON.parse(
      execFileSync("okf", ["show", bundle, "docs/engine"], { encoding: "utf8" }),
    );
    assert.match(shown.concept.body, /Preserve records/);
    rmSync(join(root, "docs/engine.md"));
    buildKnowledge(root);
    assert.equal(documentPaths(root).length, 3);
    assert.throws(() => readFileSync(join(bundle, "docs/engine.md")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("missing metadata rejects additions", () => {
  const root = fixture();
  try {
    writeFileSync(join(root, "new.md"), "# Missing frontmatter\n");
    assert.throws(() => buildKnowledge(root));
    writeFileSync(
      join(root, "new.md"),
      "---\ntype: Reference\n---\n\n# No retrieval description\n",
    );
    assert.throws(() => buildKnowledge(root), /frontmatter/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

// These are task-sized retrieval checks, not a second documentation inventory.
test("repository tasks find focused topics", () => {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
  buildKnowledge(root);
  const bundle = join(root, ".knowledge");
  const tasks = [
    {
      flags: ["--tag", "engine", "--type", "Reference"],
      ids: ["conformance/semantics", "docs/design/engine"],
    },
    {
      flags: ["--tag", "bench", "--type", "Playbook", "--text", "current-head"],
      ids: ["docs/development/continuous-benchmarks"],
    },
    {
      flags: ["--tag", "docs", "--text", "small current corpus"],
      ids: ["docs/development/knowledge"],
    },
    { flags: ["--type", "Evidence", "--tag", "performance"], ids: ["docs/design/port"] },
  ];
  for (const task of tasks) {
    const result = JSON.parse(
      execFileSync("okf", ["search", bundle, ...task.flags], { encoding: "utf8" }),
    );
    const ids = result.results.map((entry: { id: string }) => entry.id);
    for (const id of task.ids) assert.ok(ids.includes(id), `Missing task topic: ${id}`);
    assert.ok(result.results.every((entry: object) => !("body" in entry)));
  }
  const plan = JSON.parse(
    execFileSync("okf", ["show", bundle, "docs/proposals/python-binding"], { encoding: "utf8" }),
  );
  assert.equal(plan.concept.status, "draft");
  const validation = JSON.parse(execFileSync("okf", ["validate", bundle], { encoding: "utf8" }));
  assert.equal(validation.errors, 0);
  // v0.5.2 calls links to source files broken concepts; missing Markdown is not excused.
  assert.ok(
    validation.findings.every(
      (finding: { rule: string; message: string }) =>
        finding.rule === "okf/links/broken" && !finding.message.includes(".md"),
    ),
  );
});
