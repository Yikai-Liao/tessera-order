// SPDX-License-Identifier: BSD-2-Clause
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { createMarkdownRenderer } from "vitepress";
import config from "./config.ts";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

test("agent task links resolve to existing documents and sections", async () => {
  const md = await createMarkdownRenderer(root, config.markdown);
  const entry = readFileSync(resolve(root, "AGENTS.md"), "utf8");
  for (const [, href] of entry.matchAll(/\]\(([^)\s]+)\)/g)) {
    const [path, anchor] = href.split("#");
    const target = resolve(root, path);
    assert.ok(existsSync(target), `Missing task reference: ${href}`);
    if (anchor && target.endsWith(".md")) {
      const html = md.render(readFileSync(target, "utf8"), { path: target });
      assert.ok(html.includes(`id="${anchor}"`), `Missing task section: ${href}`);
    }
  }
});

test("repository links distinguish site pages, shared sources and code", async () => {
  const md = await createMarkdownRenderer(root, config.markdown);
  const html = md.render(
    "[semantics](../../conformance/semantics.md#space) " +
      "[engine procedure](../../CONTRIBUTING.md#engine-changes) " +
      "[feedback](continuous-benchmarks.md#read-feedback-from-a-pr) " +
      "[source](../../rust/tessera-sort/src/engine.rs)",
    { path: resolve(root, "docs/development/benchmarking.md") },
  );
  assert.match(html, /href="\/conformance\/semantics[^"#]*#space"/);
  assert.match(html, /href="\/CONTRIBUTING[^"#]*#engine-changes"/);
  assert.match(html, /href="\/development\/continuous-benchmarks[^"#]*#read-feedback-from-a-pr"/);
  assert.match(
    html,
    /href="https:\/\/github.com\/Yikai-Liao\/tessera-sort\/blob\/main\/rust\/tessera-sort\/src\/engine.rs"/,
  );
  assert.equal(
    config.rewrites["docs/development/continuous-benchmarks.md"],
    "development/continuous-benchmarks.md",
  );
});
