// SPDX-License-Identifier: BSD-2-Clause
import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { createMarkdownRenderer } from "vitepress";
import workflows, { workflowPages, workflowSource } from "../development/[workflow].paths.ts";
import config from "./config.ts";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

test("workflow pages use discoverable skill sources without copying their bodies", async () => {
  const md = await createMarkdownRenderer(root, config.markdown);
  const entry = readFileSync(join(root, "AGENTS.md"), "utf8");
  const skillDirectories = readdirSync(join(root, ".agents/skills"));
  const routes = workflows.paths();
  assert.equal(routes.length, workflowPages.size);
  for (const directory of skillDirectories) {
    const source = `.agents/skills/${directory}/SKILL.md`;
    assert.ok(workflowPages.has(source), `Skill missing its human-readable page: ${source}`);
  }
  for (const [source, page] of workflowPages) {
    const content = readFileSync(join(root, source), "utf8");
    const route = routes.find((route) => route.params.workflow === basename(page, ".md"));
    assert.equal(route?.content, content);
    assert.ok(entry.includes(`(${source})`), `Skill not reachable from the agent entry: ${source}`);
    assert.equal(workflowSource(page), source);
    assert.equal(workflowSource(`docs/${page}`), source);
    assert.equal(config.rewrites[`docs/${page}`], page);

    const env: { path: string; realPath: string; frontmatter?: Record<string, unknown> } = {
      path: join(root, page),
      realPath: join(root, "docs/development/[workflow].md"),
    };
    const template = readFileSync(join(root, "docs/development/[workflow].md"), "utf8");
    const html = md.render(template.replace("<!-- @content -->", content), env);
    assert.equal(env.frontmatter?.name, basename(dirname(source)));
    const description = env.frontmatter?.description;
    assert.ok(
      typeof description === "string" && description.length > 0 && description.length <= 1024,
    );
    assert.match(content, /SPDX-License-Identifier: BSD-2-Clause/);
    for (const [, href] of content.matchAll(/\]\(([^)\s]+)\)/g)) {
      const [path, anchor] = href.split("#");
      const target = resolve(root, dirname(source), path);
      assert.ok(existsSync(target), `Missing workflow reference: ${href}`);
      if (anchor && target.endsWith(".md")) {
        const targetHtml = md.render(readFileSync(target, "utf8"), { path: target });
        assert.ok(targetHtml.includes(`id="${anchor}"`), `Missing workflow anchor: ${href}`);
      }
    }
    assert.doesNotMatch(html, /href="(?:\.\.\/|\.\/)/);
    if (source.includes("tessera-engine/")) {
      assert.match(html, /href="\/design\/engine/);
      assert.match(html, /href="\/conformance\/semantics/);
      assert.match(
        html,
        /href="https:\/\/github.com\/Yikai-Liao\/tessera-sort\/blob\/main\/.github\/workflows\/rust-ci.yml"/,
      );
    }
  }
  assert.equal(workflowSource("development/benchmarking.md"), undefined);
});
