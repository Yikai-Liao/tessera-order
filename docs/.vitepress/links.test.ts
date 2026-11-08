// SPDX-License-Identifier: BSD-2-Clause
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { createMarkdownRenderer } from "vitepress";
import config from "./config.ts";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

/** Resolve inline local links in a task index or its directly linked topic. */
function localLinks(source: string) {
  return [...readFileSync(source, "utf8").matchAll(/\]\(([^)\s]+)\)/g)]
    .map((match) => match[1])
    .filter((href) => !/^(?:[a-z][a-z0-9+.-]*:|\/\/)/i.test(href))
    .map((href) => {
      const [path, anchor] = href.split("#");
      return { href, anchor, target: path ? resolve(dirname(source), path) : source };
    });
}

test("task indexes and their topics link to existing documents and sections", async () => {
  const md = await createMarkdownRenderer(root, config.markdown);
  const indexes = ["AGENTS.md", "docs/README.md", "bench/README.md"].map((path) =>
    resolve(root, path),
  );
  const topics = new Set(indexes);
  for (const index of indexes) {
    for (const { target } of localLinks(index)) {
      if (target.endsWith(".md")) topics.add(target);
    }
  }
  const rendered = new Map<string, string>();
  for (const source of topics) {
    for (const { href, target, anchor } of localLinks(source)) {
      assert.ok(existsSync(target), `Missing reference in ${source}: ${href}`);
      if (anchor && target.endsWith(".md")) {
        if (!rendered.has(target))
          rendered.set(target, md.render(readFileSync(target, "utf8"), { path: target }));
        assert.ok(
          rendered.get(target)?.includes(`id="${anchor}"`),
          `Missing section in ${source}: ${href}`,
        );
      }
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
  const rewrites = config.rewrites;
  assert.ok(rewrites && typeof rewrites === "object");
  assert.equal(
    rewrites["docs/development/continuous-benchmarks.md"],
    "development/continuous-benchmarks.md",
  );
});
