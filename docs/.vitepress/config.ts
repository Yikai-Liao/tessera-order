// SPDX-License-Identifier: BSD-2-Clause
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createMarkdownRenderer, defineConfig, type MarkdownRenderer } from "vitepress";
import { renderComparisonMarkdown } from "./comparisonMarkdown.ts";
import {
  addPageMetadata,
  canonicalSiteUrl,
  discoverSitePages,
  exportLlmsArtifacts,
  pageSections,
  REPOSITORY_URL,
  resolveRepositoryLink,
} from "./llms.ts";
import { loadComparisonSnapshot } from "./theme/components/comparisonSnapshot.data.ts";

// The site renders the Markdown files of the repository in place, so the
// same files read correctly on GitHub, in an editor, and on the site. Page
// discovery and link resolution live in `llms.ts` so HTML and `.md` exports
// share one authority.
const vitepressDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(vitepressDir, "../..");
const base = process.env.DOCS_BASE ?? "/";
const markdown = { math: true, config: repositoryLinks };
// Prime VitePress's shared renderer cache before `discoverSitePages` calls it without site plugins.
await createMarkdownRenderer(repoRoot, markdown, base);
const catalog = await discoverSitePages(repoRoot);
const sections = pageSections(catalog.pages);
const sidebar = (ids: string[]) =>
  sections
    .filter((s) => ids.includes(s.id))
    .map((s) => ({
      text: s.text,
      items: s.pages.map((p) => ({
        text: p.sitePath === "benchmarks/implementations.md" ? "vs Other Sorts" : p.title,
        link: `/${p.sitePath.replace(/index\.md$/, "").replace(/\.md$/, "")}`,
      })),
    }));
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
        token.attrSet("href", resolveRepositoryLink(repoRoot, source, href));
        // A missing target is left alone, so the dead-link check reports it.
      }
    };
    visit(state.tokens);
  });
}

export default defineConfig({
  title: "Tessera",
  description: "Stable radix ordering with O(√n) auxiliary memory",
  lang: "en-US",
  // GitHub Pages serves the project under /<repository>/; the workflow sets it.
  base,
  srcDir: "..",
  srcExclude: catalog.srcExclude,
  rewrites: catalog.rewrites,
  cleanUrls: true,
  lastUpdated: true,
  sitemap: {
    hostname: siteUrl,
  },
  head: [["link", { rel: "icon", type: "image/svg+xml", href: `${base}logo.svg` }]],
  markdown,
  transformPageData: (page) => addPageMetadata(page, repoRoot, siteUrl),
  async buildEnd(siteConfig) {
    const snapshot = await loadComparisonSnapshot();
    await exportLlmsArtifacts({
      repoRoot,
      outDir: siteConfig.outDir,
      siteUrl,
      catalog,
      renderComparison: snapshot ? (view) => renderComparisonMarkdown(snapshot, view) : undefined,
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
      { text: "Benchmarks", link: "/benchmarks/implementations", activeMatch: "^/benchmarks/" },
      { text: "How it works", link: "/explain/", activeMatch: "^/explain/" },
      {
        text: "Internals",
        link: "/design/engine",
        activeMatch: "^/(design|development)/|^/(CONTRIBUTING|RELEASING|ROADMAP)",
      },
    ],
    sidebar: {
      "/guide/": sidebar(["guide"]),
      "/conformance/": sidebar(["guide"]),
      "/explain/": sidebar(["explain"]),
      "/benchmarks/": sidebar(["benchmarks"]),
      "/": sidebar(["design", "development", "proposals"]),
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
