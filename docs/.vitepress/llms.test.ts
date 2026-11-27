// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, readFile, rm } from "node:fs/promises";
import { join, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import {
  buildPageHeadMeta,
  COMPARISON_JSON_URL,
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

test("canonicalSiteUrl respects explicit DOCS_BASE and defaults to GitHub Pages path", () => {
  assert.equal(canonicalSiteUrl(undefined), "https://yikai-liao.github.io/tessera-order/");
  assert.equal(canonicalSiteUrl("/tessera-order/"), "https://yikai-liao.github.io/tessera-order/");
  assert.equal(canonicalSiteUrl("/"), "https://yikai-liao.github.io/");
  assert.equal(canonicalSiteUrl("/fork-repo"), "https://yikai-liao.github.io/fork-repo/");
});

test("renderPageMarkdown strips frontmatter/scripts/Vue tags, expands home hero, preserves LaTeX and code blocks, and rewrites links", () => {
  const catalog = discoverSitePages(repoRoot);
  const sample = `---
layout: home
hero:
  name: Tessera
  text: Stable radix sorting in O(√n) extra memory
  tagline: Sorts records by a fixed-width key without an input-sized buffer.
features:
  - title: Small memory
    details: Scratch grows as √n.
    link: /explain/block-partition
---

<!-- SPDX-License-Identifier: BSD-2-Clause -->

<script setup lang="ts">
import ImplementationComparison from "./.vitepress/theme/components/ImplementationComparison.vue";
</script>

Inline math $O(\\sqrt{n})$ and display math:

$$
B = \\lceil \\sqrt{n} \\rceil
$$

Read [semantics](../conformance/semantics.md#key-encoding), [guide](guide/getting-started.md), [task index](README.md), and [rows template](../bench/plot/rows.ts).

<ImplementationComparison view="overview" />

<PerformanceHistory />

<BlockPartitionDemo />

Keep code examples verbatim:

\`\`\`vue
<script setup>
import PerformanceHistory from "./PerformanceHistory.vue";
</script>
<PerformanceHistory />
\`\`\`

And inline code \`<BlockPartitionDemo />\` plus \`[not a link](README.md)\`.
`;

  const rendered = renderPageMarkdown({
    raw: sample,
    sourceAbsPath: resolve(repoRoot, "docs/index.md"),
    repoRoot,
    siteUrl,
    sitePages: catalog.sitePageSet,
    renderComparison: (view) =>
      `### Comparison (${view})\n\n| Sort | Speed |\n|---|---|\n| Tessera | 2.5× |`,
  });

  // Home frontmatter expanded cleanly and YAML delimiters stripped.
  assert.match(rendered, /^# Tessera — Stable radix sorting in O\(√n\) extra memory/);
  assert.match(rendered, /Sorts records by a fixed-width key without an input-sized buffer\./);
  assert.match(
    rendered,
    /- \*\*Small memory:\*\* Scratch grows as √n\. \(\[Details\]\(https:\/\/yikai-liao\.github\.io\/tessera-order\/explain\/block-partition\.md\)\)/,
  );
  assert.ok(!rendered.includes("layout: home"));
  assert.ok(!rendered.includes("SPDX-License-Identifier"));

  // Top-level script & standalone interactive Vue tags stripped; comparison injected; history linked.
  assert.ok(!rendered.includes("import ImplementationComparison"));
  assert.match(rendered, /### Comparison \(overview\)/);
  assert.ok(rendered.includes(HISTORY_DATA_URL));
  assert.ok(!rendered.includes("\n<BlockPartitionDemo />\n"));

  // Code blocks and inline code preserved verbatim.
  assert.ok(rendered.includes("import PerformanceHistory from"));
  assert.ok(rendered.includes("`<BlockPartitionDemo />`"));
  assert.ok(rendered.includes("`[not a link](README.md)`"));

  // LaTeX preserved verbatim.
  assert.ok(rendered.includes("$O(\\sqrt{n})$"));
  assert.ok(rendered.includes("B = \\lceil \\sqrt{n} \\rceil"));

  // Links rewritten to canonical `.md` site mirrors or GitHub `blob` URLs.
  assert.ok(
    rendered.includes(
      "[semantics](https://yikai-liao.github.io/tessera-order/conformance/semantics.md#key-encoding)",
    ),
  );
  assert.ok(
    rendered.includes(
      "[guide](https://yikai-liao.github.io/tessera-order/guide/getting-started.md)",
    ),
  );
  assert.ok(rendered.includes(`[task index](${REPOSITORY_URL}/blob/main/docs/README.md)`));
  assert.ok(rendered.includes(`[rows template](${REPOSITORY_URL}/blob/main/bench/plot/rows.ts)`));
});

test("renderPageMarkdown falls back to comparison.json link when comparison snapshot is unavailable", () => {
  const catalog = discoverSitePages(repoRoot);
  const rendered = renderPageMarkdown({
    raw: "# Comparison\n\n<ImplementationComparison />\n",
    sourceAbsPath: resolve(repoRoot, "docs/benchmarks/implementations.md"),
    repoRoot,
    siteUrl,
    sitePages: catalog.sitePageSet,
    renderComparison: () => null,
  });
  assert.ok(rendered.includes(COMPARISON_JSON_URL));
  assert.ok(!rendered.includes("<ImplementationComparison"));
});

test("exportLlmsArtifacts emits clean mirrors, tiered llms.txt, user-only llms-full.txt, and algorithm llms-explain.txt", async (t) => {
  const target = fileURLToPath(new URL("../../target/", import.meta.url));
  await mkdir(target, { recursive: true });
  const outDir = await mkdtemp(join(target, "docs-llms-"));
  t.after(() => rm(outDir, { recursive: true, force: true }));

  const result = await exportLlmsArtifacts({
    repoRoot,
    outDir,
    siteUrl,
    renderComparison: (view) => `Benchmark comparison summary (${view}).`,
  });

  assert.ok(
    result.sitePaths.length >= 25,
    `expected all site pages, got ${result.sitePaths.length}`,
  );

  // Every emitted `.md` file exists and has no leaked script/Vue tags or broken relative `.md` links.
  for (const path of result.sitePaths) {
    const file = join(outDir, path);
    assert.ok(existsSync(file), `missing emitted mirror ${path}`);
    const content = await readFile(file, "utf8");
    assert.ok(content.trim().length > 0, `empty emitted mirror ${path}`);
    assert.ok(!content.startsWith("---\n"), `frontmatter leaked in ${path}`);

    // Strip fenced and indented code blocks before checking for leaked tags or relative links.
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
      if (line.startsWith("\t") || line.startsWith("    ")) continue;
      proseLines.push(line);
    }
    const prose = proseLines.join("\n");
    assert.ok(!/^<script\b/m.test(prose), `<script> leaked in ${path}`);
    assert.ok(
      !/^<(ImplementationComparison|PerformanceHistory|BlockPartitionDemo|CountingPassDemo|BenchmarkFigure)\b/m.test(
        prose,
      ),
      `Vue component tag leaked in ${path}`,
    );

    // Every Markdown link in prose outside inline code must be absolute (`https://`) or same-page `#anchor`.
    const withoutInlineCode = prose.replace(/`[^`]+`/g, "");
    for (const match of withoutInlineCode.matchAll(/\[[^\]]+\]\(([^)\s]+)[^)]*\)/g)) {
      const href = match[1];
      assert.ok(
        href.startsWith("https://") || href.startsWith("http://") || href.startsWith("#"),
        `unrewritten relative link "${href}" in ${path}`,
      );
    }
  }

  // `llms.txt` structure: required sections, Optional separates internal/proposals, every referenced `.md` exists.
  assert.ok(result.llmsTxt.includes("## Using Tessera"));
  assert.ok(result.llmsTxt.includes("## How Tessera Works"));
  assert.ok(result.llmsTxt.includes("## Optional"));

  const optionalIndex = result.llmsTxt.indexOf("## Optional");
  const beforeOptional = result.llmsTxt.slice(0, optionalIndex);
  const inOptional = result.llmsTxt.slice(optionalIndex);

  assert.ok(beforeOptional.includes(`${siteUrl}guide/getting-started.md`));
  assert.ok(beforeOptional.includes(`${siteUrl}conformance/semantics.md`));
  assert.ok(beforeOptional.includes(`${siteUrl}explain/block-partition.md`));
  assert.ok(!beforeOptional.includes("design/evidence.md"));
  assert.ok(!beforeOptional.includes("operation-extensions-proposal.md"));
  assert.ok(!beforeOptional.includes("python-binding.md"));

  assert.ok(inOptional.includes(`${siteUrl}design/engine.md`));
  assert.ok(inOptional.includes(`${siteUrl}design/evidence.md`));
  assert.ok(inOptional.includes(`${siteUrl}design/operation-extensions-proposal.md`));
  assert.ok(inOptional.includes(`${siteUrl}design/python-binding.md`));
  assert.ok(inOptional.includes(`${siteUrl}ROADMAP.md`));

  // Every `.md` URL referenced in `llms.txt` corresponds to an emitted file in `outDir`.
  const mdLinks = [...result.llmsTxt.matchAll(/\]\((https:\/\/[^)\s]+\.md)\)/g)].map((m) => m[1]);
  assert.ok(mdLinks.length === result.sitePaths.length, "every site page should be indexed once");
  for (const url of mdLinks) {
    assert.ok(url.startsWith(siteUrl), `unexpected link origin: ${url}`);
    const rel = url.slice(siteUrl.length);
    assert.ok(existsSync(join(outDir, rel)), `llms.txt links missing file ${rel}`);
  }

  // `llms-full.txt` contains external user core and strictly excludes internal evidence & proposals.
  assert.ok(result.llmsFullTxt.includes(`Source: ${siteUrl}index.md`));
  assert.ok(result.llmsFullTxt.includes(`Source: ${siteUrl}guide/getting-started.md`));
  assert.ok(result.llmsFullTxt.includes(`Source: ${siteUrl}conformance/semantics.md`));
  assert.ok(result.llmsFullTxt.includes(`Source: ${siteUrl}benchmarks/implementations.md`));
  assert.ok(result.llmsFullTxt.includes(`Source: ${siteUrl}benchmarks/index.md`));
  assert.ok(result.llmsFullTxt.includes(`Source: ${siteUrl}explain/index.md`));
  assert.ok(!result.llmsFullTxt.includes(`Source: ${siteUrl}design/evidence.md`));
  assert.ok(!result.llmsFullTxt.includes(`Source: ${siteUrl}design/engine.md`));
  assert.ok(!result.llmsFullTxt.includes("operation-extensions-proposal.md\n"));
  assert.ok(!result.llmsFullTxt.includes(`Source: ${siteUrl}design/python-binding.md`));

  // `llms-explain.txt` includes overview and chapters 1–7 in reading order.
  const explainMarkers = [
    "explain/index.md",
    "explain/radix-sort.md",
    "explain/keys.md",
    "explain/routing.md",
    "explain/block-partition.md",
    "explain/tiles.md",
    "explain/safety.md",
    "explain/operations.md",
  ].map((p) => result.llmsExplainTxt.indexOf(`Source: ${siteUrl}${p}`));
  for (let i = 0; i < explainMarkers.length; i++) {
    assert.ok(explainMarkers[i] >= 0, `missing explain chapter ${i}`);
    if (i > 0)
      assert.ok(explainMarkers[i] > explainMarkers[i - 1], "explain chapters out of order");
  }
});

test("per-page metadata produces distinct non-empty descriptions and valid canonical/alternate head tags", async () => {
  const catalog = discoverSitePages(repoRoot);
  const descriptions = new Map<string, string>();

  for (const entry of catalog.pages) {
    const raw = await readFile(resolve(repoRoot, entry.sourcePath), "utf8");
    const meta = extractPageMetadata(raw);
    assert.ok(meta.description.length >= 20, `too-short description for ${entry.sitePath}`);
    assert.ok(!meta.description.includes("]("), `unstripped Markdown link in ${entry.sitePath}`);
    assert.ok(!meta.description.includes("**"), `unstripped bold syntax in ${entry.sitePath}`);
    descriptions.set(entry.sitePath, meta.description);

    const headMeta = buildPageHeadMeta({
      relativePath: entry.sitePath,
      filePath: entry.sourcePath,
      title: meta.title,
      description: "",
      frontmatter: {},
      srcDir: repoRoot,
      siteUrl,
    });
    assert.equal(headMeta.description, meta.description);
    assert.ok(
      headMeta.head.some(
        ([tag, attrs]) =>
          tag === "link" && attrs.rel === "canonical" && attrs.href.startsWith(siteUrl),
      ),
    );
    assert.ok(
      headMeta.head.some(
        ([tag, attrs]) =>
          tag === "link" &&
          attrs.rel === "alternate" &&
          attrs.type === "text/markdown" &&
          attrs.href === `${siteUrl}${entry.sitePath}`,
      ),
    );
    assert.ok(
      headMeta.head.some(([tag, attrs]) => tag === "meta" && attrs.property === "og:title"),
    );
    assert.ok(
      headMeta.head.some(
        ([tag, attrs]) =>
          tag === "meta" &&
          attrs.property === "og:description" &&
          attrs.content === meta.description,
      ),
    );
  }

  // Every page has a distinct description (no two pages fall back to an identical boilerplate string).
  assert.equal(new Set(descriptions.values()).size, catalog.pages.length);
  assert.ok(
    descriptions.get("explain/safety.md")?.includes("Box<T>"),
    "inline code generics should stay intact in descriptions",
  );
  assert.ok(
    !descriptions.get("benchmarks/index.md")?.endsWith(":"),
    "short list lead-in lines should not become the page description",
  );
});
