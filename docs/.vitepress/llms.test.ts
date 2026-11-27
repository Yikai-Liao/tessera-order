// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, readFile, rm } from "node:fs/promises";
import { join, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { COMPARISON_DATA_URL } from "./comparisonMarkdown.ts";
import {
  buildPageHeadMeta,
  canonicalSiteUrl,
  discoverSitePages,
  exportLlmsArtifacts,
  extractPageMetadata,
  HISTORY_DATA_URL,
  REPOSITORY_URL,
  renderPageMarkdown,
} from "./llms.ts";

const repoRoot = fileURLToPath(new URL("../..", import.meta.url));
const siteUrl = canonicalSiteUrl("/tessera-order/");

test("canonicalSiteUrl normalizes explicit DOCS_BASE and defaults to GitHub Pages path", () => {
  assert.equal(canonicalSiteUrl(undefined), "https://yikai-liao.github.io/tessera-order/");
  assert.equal(canonicalSiteUrl("/"), "https://yikai-liao.github.io/");
  assert.equal(canonicalSiteUrl("/fork-repo"), "https://yikai-liao.github.io/fork-repo/");
});

test("renderPageMarkdown strips noise, expands hero, preserves code/LaTeX, and rewrites links", () => {
  const catalog = discoverSitePages(repoRoot);
  const sample = `---
layout: home
hero:
  name: Tessera
  text: Stable radix sorting in O(√n) extra memory
  tagline: Sorts records by a fixed-width key without an input-sized buffer.
---

<!-- SPDX-License-Identifier: BSD-2-Clause -->

<script setup lang="ts">
import ImplementationComparison from "./.vitepress/theme/components/ImplementationComparison.vue";
</script>

<div class="tsr-home-grid">

Inline math $O(\\sqrt{n})$ and display math:

$$
B = \\lceil \\sqrt{n} \\rceil
$$

Read [semantics](../conformance/semantics.md#key-encoding), [guide](guide/getting-started.md), [task index](README.md), and [rows](../bench/plot/rows.ts).

</div>

<ImplementationComparison view="headlines" />

<ImplementationComparison view="overview" />

<PerformanceHistory />

<BlockPartitionDemo />

\`\`\`vue
<script setup>
import PerformanceHistory from "./PerformanceHistory.vue";
</script>
<PerformanceHistory />
\`\`\`

Inline \`<BlockPartitionDemo />\` and \`[not a link](README.md)\`.
`;

  const rendered = renderPageMarkdown({
    raw: sample,
    sourceAbsPath: resolve(repoRoot, "docs/index.md"),
    repoRoot,
    siteUrl,
    sitePages: catalog.sitePageSet,
    renderComparison: (view) => `Comparison(${view})`,
  });

  assert.match(rendered, /^# Tessera — Stable radix sorting in O\(√n\) extra memory/);
  assert.ok(rendered.includes("Sorts records by a fixed-width key without an input-sized buffer."));
  assert.ok(!rendered.includes("layout: home"));
  assert.ok(!rendered.includes("SPDX-License-Identifier"));
  assert.ok(!rendered.includes("tsr-home-grid"));
  assert.ok(!rendered.includes("import ImplementationComparison"));
  assert.ok(rendered.includes("Comparison(headlines)"));
  assert.ok(rendered.includes("Comparison(overview)"));
  assert.ok(rendered.includes(HISTORY_DATA_URL));
  assert.ok(!rendered.includes("\n<BlockPartitionDemo />\n"));

  // Code, inline code, and LaTeX stay verbatim.
  assert.ok(rendered.includes("import PerformanceHistory from"));
  assert.ok(rendered.includes("`<BlockPartitionDemo />`"));
  assert.ok(rendered.includes("`[not a link](README.md)`"));
  assert.ok(rendered.includes("$O(\\sqrt{n})$"));
  assert.ok(rendered.includes("B = \\lceil \\sqrt{n} \\rceil"));

  // Links rewrite to canonical `.md` site mirrors or GitHub `blob` URLs.
  assert.ok(rendered.includes(`[semantics](${siteUrl}conformance/semantics.md#key-encoding)`));
  assert.ok(rendered.includes(`[guide](${siteUrl}guide/getting-started.md)`));
  assert.ok(rendered.includes(`[task index](${REPOSITORY_URL}/blob/main/docs/README.md)`));
  assert.ok(rendered.includes(`[rows](${REPOSITORY_URL}/blob/main/bench/plot/rows.ts)`));
});

test("renderPageMarkdown degrades cleanly when comparison snapshot is unavailable", () => {
  const catalog = discoverSitePages(repoRoot);
  const rendered = renderPageMarkdown({
    raw: '# Benchmarks\n\n<ImplementationComparison view="headlines" />\n\n<ImplementationComparison />\n',
    sourceAbsPath: resolve(repoRoot, "docs/benchmarks/implementations.md"),
    repoRoot,
    siteUrl,
    sitePages: catalog.sitePageSet,
    renderComparison: () => null,
  });
  assert.ok(rendered.includes(COMPARISON_DATA_URL));
  assert.ok(!rendered.includes("<ImplementationComparison"));
});

test("exportLlmsArtifacts emits clean mirrors, tiered llms.txt, user-only llms-full.txt, and ordered llms-explain.txt", async (t) => {
  const target = fileURLToPath(new URL("../../target/", import.meta.url));
  await mkdir(target, { recursive: true });
  const outDir = await mkdtemp(join(target, "docs-llms-"));
  t.after(() => rm(outDir, { recursive: true, force: true }));

  const result = await exportLlmsArtifacts({
    repoRoot,
    outDir,
    siteUrl,
    renderComparison: (view) => `Comparison(${view}).`,
  });

  assert.ok(result.sitePaths.length >= 25);

  for (const path of result.sitePaths) {
    const content = await readFile(join(outDir, path), "utf8");
    assert.ok(content.trim().length > 0, `empty mirror ${path}`);
    assert.ok(!content.startsWith("---\n"), `frontmatter leaked in ${path}`);

    // Strip fenced and indented code blocks before scanning prose for leaks or relative links.
    const proseLines: string[] = [];
    let fence: string | null = null;
    for (const line of content.split("\n")) {
      const m = line.match(/^\s*(`{3,}|~{3,})/);
      if (fence) {
        if (line.trim().startsWith(fence)) fence = null;
        continue;
      }
      if (m) {
        fence = m[1][0].repeat(m[1].length);
        continue;
      }
      if (!line.startsWith("\t") && !line.startsWith("    ")) proseLines.push(line);
    }
    const prose = proseLines.join("\n");
    assert.ok(!/^<script\b|^<\/?div\b/m.test(prose), `raw HTML/script leaked in ${path}`);
    assert.ok(
      !/^<(ImplementationComparison|PerformanceHistory|BlockPartitionDemo|CountingPassDemo|BenchmarkFigure|PipelineMap)\b/m.test(
        prose,
      ),
      `Vue tag leaked in ${path}`,
    );
    for (const match of prose.replace(/`[^`]+`/g, "").matchAll(/\[[^\]]+\]\(([^)\s]+)[^)]*\)/g)) {
      const href = match[1];
      assert.ok(
        href.startsWith("https://") || href.startsWith("http://") || href.startsWith("#"),
        `unrewritten link "${href}" in ${path}`,
      );
    }
  }

  // `llms.txt` tiers: internal docs and proposals stay under Optional, every site page indexed once.
  const optionalIndex = result.llmsTxt.indexOf("## Optional");
  assert.ok(optionalIndex > 0);
  const coreIndex = result.llmsTxt.slice(0, optionalIndex);
  const optionalSection = result.llmsTxt.slice(optionalIndex);
  assert.ok(coreIndex.includes(`${siteUrl}guide/getting-started.md`));
  assert.ok(coreIndex.includes(`${siteUrl}explain/block-partition.md`));
  assert.ok(!coreIndex.includes("design/evidence.md"));
  assert.ok(!coreIndex.includes("operation-extensions-proposal.md"));
  assert.ok(optionalSection.includes(`${siteUrl}design/engine.md`));
  assert.ok(optionalSection.includes(`${siteUrl}design/evidence.md`));
  assert.ok(optionalSection.includes(`${siteUrl}ROADMAP.md`));

  const indexedUrls = [...result.llmsTxt.matchAll(/\]\((https:\/\/[^)\s]+\.md)\)/g)].map(
    (m) => m[1],
  );
  assert.equal(indexedUrls.length, result.sitePaths.length);
  for (const url of indexedUrls) {
    assert.ok(existsSync(join(outDir, url.slice(siteUrl.length))), `missing file for ${url}`);
  }

  // `llms-full.txt` bundles user core only; `llms-explain.txt` keeps chapters 1..7 in order.
  assert.ok(result.llmsFullTxt.includes(`Source: ${siteUrl}guide/getting-started.md`));
  assert.ok(result.llmsFullTxt.includes(`Source: ${siteUrl}benchmarks/implementations.md`));
  assert.ok(!result.llmsFullTxt.includes(`Source: ${siteUrl}design/evidence.md`));
  assert.ok(!result.llmsFullTxt.includes(`Source: ${siteUrl}design/python-binding.md`));

  const explainOrder = [
    "explain/index.md",
    "explain/radix-sort.md",
    "explain/keys.md",
    "explain/routing.md",
    "explain/block-partition.md",
    "explain/tiles.md",
    "explain/safety.md",
    "explain/operations.md",
  ].map((p) => result.llmsExplainTxt.indexOf(`Source: ${siteUrl}${p}`));
  for (let i = 0; i < explainOrder.length; i++) {
    assert.ok(explainOrder[i] >= 0, `missing explain chapter ${i}`);
    if (i > 0) assert.ok(explainOrder[i] > explainOrder[i - 1], "explain chapters out of order");
  }
});

test("per-page metadata yields distinct clean descriptions and canonical/alternate head tags", async () => {
  const catalog = discoverSitePages(repoRoot);
  const descriptions = new Map<string, string>();

  for (const entry of catalog.pages) {
    const meta = extractPageMetadata(await readFile(resolve(repoRoot, entry.sourcePath), "utf8"));
    assert.ok(meta.description.length >= 20, `short description for ${entry.sitePath}`);
    assert.ok(!meta.description.includes("](") && !meta.description.includes("**"));
    descriptions.set(entry.sitePath, meta.description);

    const head = buildPageHeadMeta({
      relativePath: entry.sitePath,
      filePath: entry.sourcePath,
      title: meta.title,
      description: "",
      frontmatter: {},
      srcDir: repoRoot,
      siteUrl,
    });
    assert.equal(head.description, meta.description);
    assert.ok(head.head.some(([t, a]) => t === "link" && a.rel === "canonical"));
    assert.ok(
      head.head.some(
        ([t, a]) =>
          t === "link" &&
          a.rel === "alternate" &&
          a.type === "text/markdown" &&
          a.href === `${siteUrl}${entry.sitePath}`,
      ),
    );
  }

  assert.equal(new Set(descriptions.values()).size, catalog.pages.length);
  assert.ok(descriptions.get("explain/safety.md")?.includes("Box<T>"));
});
