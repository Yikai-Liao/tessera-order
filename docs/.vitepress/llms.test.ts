// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { createMarkdownRenderer } from "vitepress";
import { COMPARISON_DATA_URL } from "./comparisonMarkdown.ts";
import {
  addPageMetadata,
  canonicalSiteUrl,
  discoverSitePages,
  explainReadingOrder,
  exportLlmsArtifacts,
  extractPageMetadata,
  HISTORY_DATA_URL,
  pageSections,
  REPOSITORY_URL,
  renderPageMarkdown,
} from "./llms.ts";

const repoRoot = fileURLToPath(new URL("../..", import.meta.url));
const siteUrl = canonicalSiteUrl("/tessera-order/");
const catalog = await discoverSitePages(repoRoot);
const context = {
  repoRoot,
  siteUrl,
  sitePages: catalog.sitePageSet,
  sourceAbsPath: resolve(repoRoot, "docs/index.md"),
};

test("mirrors use VitePress syntax while preserving code, math and reference examples", async () => {
  const raw = `---
layout: home
hero:
  name: Tessera
  text: Stable radix sorting
  tagline: >-
    Sort records without
    an input-sized buffer.
---
<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup>
import ImplementationComparison from "./Comparison.vue";
</script>
<div class="wrapper">
<ImplementationComparison view="headlines" />
<ImplementationComparison view="overview" />
<PipelineMap />
</div>

Inline math $O(\\sqrt{n})$.

$$
B = \\lceil \\sqrt{n} \\rceil
$$

Read [nested **label**](<guide/getting-started.md> "Guide"), [semantics](../conformance/semantics.md#key-encoding), [index](README.md), and [rows](../bench/plot/rows.ts).

> [quoted](guide/getting-started.md)

\`[reference]: guide/getting-started.md "Guide"\`

[reference]: guide/getting-started.md "Guide"

Read [Guide][reference]. Inline \`[not a link](README.md)\`.

<PerformanceHistory />
<BlockPartitionDemo />

\`\`\`vue
<script setup>
import PerformanceHistory from "./PerformanceHistory.vue";
</script>
<PerformanceHistory />
\`\`\`
`;
  const rendered = await renderPageMarkdown({
    ...context,
    raw,
    renderComparison: (view) => `Comparison(${view})`,
  });
  for (const fragment of [
    "# Tessera — Stable radix sorting",
    "Sort records without an input-sized buffer.",
    "Comparison(headlines)",
    "Comparison(overview)",
    "| Workload | Runs | Skips |",
    HISTORY_DATA_URL,
    "$O(\\sqrt{n})$",
    "B = \\lceil \\sqrt{n} \\rceil",
    "import PerformanceHistory from",
    '`[reference]: guide/getting-started.md "Guide"`',
    "`[not a link](README.md)`",
    `](<${siteUrl}guide/getting-started.md> "Guide")`,
    `[reference]: ${siteUrl}guide/getting-started.md "Guide"`,
    `> [quoted](${siteUrl}guide/getting-started.md)`,
    `[semantics](${siteUrl}conformance/semantics.md#key-encoding)`,
    `[index](${REPOSITORY_URL}/blob/main/docs/README.md)`,
    `[rows](${REPOSITORY_URL}/blob/main/bench/plot/rows.ts)`,
  ])
    assert.ok(rendered.includes(fragment), fragment);
  assert.doesNotMatch(
    rendered,
    /layout: home|SPDX-License|class="wrapper"|import ImplementationComparison|<ImplementationComparison|<PipelineMap|<BlockPartitionDemo/,
  );
  assert.deepEqual(await extractPageMetadata(raw), {
    title: "Tessera — Stable radix sorting",
    description: "Sort records without an input-sized buffer.",
  });
  assert.equal(canonicalSiteUrl(), "https://yikai-liao.github.io/tessera-order/");
  assert.equal(canonicalSiteUrl("/"), "https://yikai-liao.github.io/");
  assert.equal(canonicalSiteUrl("/fork"), "https://yikai-liao.github.io/fork/");
});

test("missing comparison data exports a usable data link", async () => {
  const rendered = await renderPageMarkdown({
    ...context,
    raw: '<ImplementationComparison view="headlines" />\n\n<ImplementationComparison />\n',
  });
  assert.ok(rendered.includes(COMPARISON_DATA_URL));
  assert.doesNotMatch(rendered, /<ImplementationComparison/);
});

test("every page has a clean mirror, unique metadata and exactly one audience tier", async (t) => {
  const outDir = await mkdtemp(join(tmpdir(), "tessera-llms-"));
  t.after(() => rm(outDir, { recursive: true, force: true }));
  const result = await exportLlmsArtifacts({
    repoRoot,
    outDir,
    siteUrl,
    catalog,
    renderComparison: (view) => `Comparison(${view})`,
  });
  const md = await createMarkdownRenderer(repoRoot);
  for (const entry of catalog.pages) {
    const content = await readFile(join(outDir, entry.sitePath), "utf8");
    assert.ok(content.trim(), entry.sitePath);
    assert.ok(!content.startsWith("---\n"), entry.sitePath);
    for (const token of md.parse(content, {})) {
      if (token.type.startsWith("html"))
        assert.doesNotMatch(token.content, /<(?:script|\/?div|[A-Z])/);
      for (const child of token.children ?? []) {
        if (child.type === "link_open")
          assert.match(child.attrGet("href")!, /^(?:https?:|#)/, entry.sitePath);
      }
    }
    const page = {
      relativePath: entry.sitePath,
      filePath: entry.sourcePath,
      title: "",
      description: "",
      frontmatter: {} as { head?: import("vitepress").HeadConfig[] },
      headers: [],
    };
    await addPageMetadata(page, repoRoot, siteUrl);
    assert.equal(page.description, entry.description);
    assert.ok(page.description.length >= 20);
    assert.ok(
      page.frontmatter.head!.some(([tag, attrs]) => tag === "link" && attrs.rel === "canonical"),
    );
    assert.ok(
      page.frontmatter.head!.some(
        ([tag, attrs]) =>
          tag === "link" &&
          attrs.type === "text/markdown" &&
          attrs.href === `${siteUrl}${entry.sitePath}`,
      ),
    );
  }
  assert.equal(new Set(catalog.pages.map((p) => p.description)).size, catalog.pages.length);
  assert.ok(
    catalog.pages.find((p) => p.sitePath === "explain/safety.md")!.description.includes("Box<T>"),
  );
  const indexedPaths = [...result.llmsTxt.matchAll(/\]\(https:\/\/[^)\s]+\.md\)/g)].map((m) =>
    m[0].slice(2, -1).slice(siteUrl.length),
  );
  assert.deepEqual(indexedPaths.toSorted(), result.sitePaths.toSorted());
  const [core, optional] = result.llmsTxt.split("## Optional");
  for (const path of [
    "guide/getting-started.md",
    "benchmarks/implementations.md",
    "conformance/semantics.md",
    "explain/index.md",
  ]) {
    assert.ok(core.includes(`${siteUrl}${path}`));
    assert.ok(result.llmsFullTxt.includes(`Source: ${siteUrl}${path}`));
  }
  for (const path of ["design/evidence.md", "design/python-binding.md", "ROADMAP.md"]) {
    assert.ok(optional.includes(`${siteUrl}${path}`));
    assert.ok(!core.includes(`${siteUrl}${path}`));
    assert.ok(!result.llmsFullTxt.includes(`Source: ${siteUrl}${path}`));
  }
  const chapters = [
    "index",
    "radix-sort",
    "keys",
    "routing",
    "block-partition",
    "tiles",
    "safety",
    "operations",
  ].map((p) => result.llmsExplainTxt.indexOf(`Source: ${siteUrl}explain/${p}.md`));
  assert.ok(
    chapters.every((position, i) => position >= 0 && (i === 0 || position > chapters[i - 1])),
  );
  // Reordering chapters in docs/explain/index.md#reading-order must reorder sections and bundles directly,
  // while cross-reference links inside descriptions must not affect chapter order.
  const swapped = explainReadingOrder(
    "## Reading order\n\n| Page | You will learn |\n|---|---|\n| [Keys](keys.md) | see [tiles](tiles.md) first |\n| [Radix](radix-sort.md) | intro |\n| [Tiles](tiles.md) | leaf |\n\n## Memory\n",
  );
  const reordered = pageSections(catalog.pages, swapped)
    .find((s) => s.id === "explain")!
    .pages.slice(0, 4)
    .map((p) => p.sitePath);
  assert.deepEqual(reordered, [
    "explain/index.md",
    "explain/keys.md",
    "explain/radix-sort.md",
    "explain/tiles.md",
  ]);
  const figureMirror = await readFile(join(outDir, "development/benchmark-figures.md"), "utf8");
  assert.ok(figureMirror.includes("Tessera 2.6×") && figureMirror.includes("fluxsort 1.5×"));
});
