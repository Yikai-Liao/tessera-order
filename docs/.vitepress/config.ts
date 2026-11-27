// SPDX-License-Identifier: BSD-2-Clause
import { existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { defineConfig, type MarkdownRenderer } from "vitepress";
import {
  buildPageHeadMeta,
  type ComparisonRenderer,
  type ComparisonView,
  canonicalSiteUrl,
  discoverSitePages,
  exportLlmsArtifacts,
  formatRepositoryUrl,
  REPOSITORY_URL,
  resolveRepositoryLink,
} from "./llms.ts";

// The site renders the Markdown files of the repository in place, so the
// same files read correctly on GitHub, in an editor, and on the site. Page
// discovery and link resolution live in `llms.ts` so HTML and `.md` exports
// share one authority.
const vitepressDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(vitepressDir, "../..");
const catalog = discoverSitePages(repoRoot);
const siteUrl = canonicalSiteUrl(process.env.DOCS_BASE);

/**
 * Turns relative links into what each reader needs: a page link becomes an
 * absolute site path, so rewrites cannot break it, and a link to source code,
 * a directory or a file outside the site goes to GitHub.
 */
function repositoryLinks(md: MarkdownRenderer) {
  md.core.ruler.push("tessera_repository_links", (state) => {
    const source: string | undefined = state.env.realPath ?? state.env.path;
    if (!source) return;
    const visit = (tokens: typeof state.tokens) => {
      for (const token of tokens) {
        if (token.children) visit(token.children);
        if (token.type !== "link_open") continue;
        const href = token.attrGet("href");
        if (!href) continue;
        const resolved = resolveRepositoryLink(repoRoot, source, href);
        if (resolved.kind === "page") {
          token.attrSet("href", `/${resolved.sitePath}${resolved.anchor}`);
        } else if (resolved.kind === "repository") {
          token.attrSet("href", formatRepositoryUrl(resolved));
        }
        // A missing target is left alone, so the dead-link check reports it.
      }
    };
    visit(state.tokens);
  });
}

interface ComparisonMarkdownModule {
  loadComparisonSnapshot?: () => Promise<unknown>;
  renderComparisonMarkdown?: (data: unknown, view: ComparisonView) => string;
  COMPARISON_DATA_URL?: string;
}

/** Loads build-time comparison snapshot rendering from `comparisonMarkdown.ts` when present. */
async function loadOptionalComparisonRenderer(): Promise<{
  renderComparison?: ComparisonRenderer;
  comparisonSourceUrl?: string;
}> {
  const modulePath = resolve(vitepressDir, "comparisonMarkdown.ts");
  if (!existsSync(modulePath)) return {};
  const mod = (await import(pathToFileURL(modulePath).href)) as ComparisonMarkdownModule;
  const comparisonSourceUrl = mod.COMPARISON_DATA_URL;
  if (!mod.loadComparisonSnapshot || !mod.renderComparisonMarkdown) {
    return { comparisonSourceUrl };
  }
  const snapshot = await mod.loadComparisonSnapshot();
  if (!snapshot) return { comparisonSourceUrl };
  const render = mod.renderComparisonMarkdown;
  return {
    renderComparison: (view) => render(snapshot, view),
    comparisonSourceUrl,
  };
}

export default defineConfig({
  title: "Tessera",
  description: "Stable radix ordering with O(√n) auxiliary memory",
  lang: "en-US",
  // GitHub Pages serves the project under /<repository>/; the workflow sets it.
  base: process.env.DOCS_BASE ?? "/",
  srcDir: "..",
  srcExclude: catalog.srcExclude,
  rewrites: catalog.rewrites,
  cleanUrls: true,
  lastUpdated: true,
  sitemap: {
    hostname: siteUrl,
  },
  head: [
    [
      "link",
      { rel: "icon", type: "image/svg+xml", href: `${process.env.DOCS_BASE ?? "/"}logo.svg` },
    ],
  ],
  markdown: {
    math: true,
    config: repositoryLinks,
  },
  transformPageData(pageData, { siteConfig }) {
    const meta = buildPageHeadMeta({
      relativePath: pageData.relativePath,
      filePath: pageData.filePath,
      title: pageData.title,
      description: pageData.description,
      frontmatter: pageData.frontmatter,
      srcDir: siteConfig.srcDir,
      siteUrl,
    });
    pageData.description = meta.description;
    pageData.frontmatter.head = meta.head;
  },
  async buildEnd(siteConfig) {
    const { renderComparison, comparisonSourceUrl } = await loadOptionalComparisonRenderer();
    await exportLlmsArtifacts({
      repoRoot,
      outDir: siteConfig.outDir,
      siteUrl,
      catalog,
      renderComparison,
      comparisonSourceUrl,
    });
  },
  vite: {
    // `srcDir` is the repository root, one level above this Vite project.
    // Vite would otherwise look for static assets in `<srcDir>/public`.
    publicDir: resolve(vitepressDir, "../public"),
    server: { fs: { allow: [repoRoot] } },
  },
  themeConfig: {
    logo: "/logo.svg",
    nav: [
      { text: "Guide", link: "/guide/getting-started", activeMatch: "^/(guide|conformance)/" },
      { text: "How it works", link: "/explain/", activeMatch: "^/explain/" },
      {
        text: "Performance",
        activeMatch: "^/benchmarks/",
        items: [
          { text: "Overview and history", link: "/benchmarks/" },
          { text: "Implementation comparison", link: "/benchmarks/implementations" },
        ],
      },
      {
        text: "Development",
        link: "/design/engine",
        activeMatch: "^/(design|development)/|^/(CONTRIBUTING|RELEASING|ROADMAP)",
      },
    ],
    sidebar: {
      "/guide/": guideSidebar(),
      "/conformance/": guideSidebar(),
      "/explain/": [
        {
          text: "How Tessera works",
          items: [
            { text: "Overview", link: "/explain/" },
            { text: "1. Radix sort in one pass", link: "/explain/radix-sort" },
            { text: "2. Keys as unsigned integers", link: "/explain/keys" },
            { text: "3. Routing an input", link: "/explain/routing" },
            { text: "4. The block partition", link: "/explain/block-partition" },
            { text: "5. Finishing tiles", link: "/explain/tiles" },
            { text: "6. Ownership and panics", link: "/explain/safety" },
            { text: "7. Beyond sorting", link: "/explain/operations" },
          ],
        },
      ],
      "/benchmarks/": [
        {
          text: "Performance",
          items: [
            { text: "Overview and history", link: "/benchmarks/" },
            { text: "Implementation comparison", link: "/benchmarks/implementations" },
          ],
        },
      ],
      "/": developmentSidebar(),
    },
    outline: { level: [2, 3] },
    search: { provider: "local" },
    editLink: {
      // A string, not a function: VitePress serializes this option for the client.
      pattern: `${REPOSITORY_URL}/edit/main/:path`,
      text: "Edit this page on GitHub",
    },
    socialLinks: [{ icon: "github", link: REPOSITORY_URL }],
    footer: {
      message: "Released under the BSD 2-Clause License.",
    },
  },
});

function guideSidebar() {
  return [
    {
      text: "User guide",
      items: [
        { text: "Getting started", link: "/guide/getting-started" },
        { text: "Ordering semantics", link: "/conformance/semantics" },
      ],
    },
  ];
}

function developmentSidebar() {
  return [
    {
      text: "Design",
      items: [
        { text: "Engine reference", link: "/design/engine" },
        { text: "Operation extensions proposal", link: "/design/operation-extensions-proposal" },
        { text: "Python binding design", link: "/design/python-binding" },
        { text: "Engine evidence", link: "/design/evidence" },
        { text: "CI scaling proposal", link: "/design/ci-scaling-proposal" },
        { text: "Comparison model proposal", link: "/design/comparison-model-proposal" },
        { text: "Benchmark tooling", link: "/design/benchmark-tooling" },
      ],
    },
    {
      text: "Development",
      items: [
        { text: "Contributing", link: "/CONTRIBUTING" },
        { text: "Benchmark protocol", link: "/development/benchmarking" },
        { text: "Benchmark figures", link: "/development/benchmark-figures" },
        { text: "Continuous benchmarks", link: "/development/continuous-benchmarks" },
        { text: "Continuous integration", link: "/development/ci" },
        { text: "Documentation site", link: "/development/docs-site" },
        { text: "Releasing", link: "/RELEASING" },
      ],
    },
    {
      text: "Planning",
      items: [{ text: "Roadmap", link: "/ROADMAP" }],
    },
  ];
}
