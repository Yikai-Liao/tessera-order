// SPDX-License-Identifier: BSD-2-Clause

/**
 * Single authority for Tessera's documentation site topology, repository link
 * resolution, per-page metadata, and build-time AI-readable Markdown exports.
 *
 * Decisions hidden:
 * - Which repository Markdown files are published pages (`SHARED_PAGES`,
 *   `NOT_PAGES`) and how source paths map to served site paths, clean URLs,
 *   and `.md` mirror URLs.
 * - How relative repository links resolve to site pages versus GitHub
 *   `blob`/`tree` URLs (shared by VitePress's Markdown renderer and `.md`
 *   mirror emission).
 * - How YAML frontmatter (including home `hero`/`features`), `<script>` blocks,
 *   SPDX comments, and standalone Vue figure tags are projected into clean
 *   Markdown while leaving code blocks and LaTeX math (`$...$` / `$$...$$`)
 *   verbatim.
 * - How pages are tiered into `/llms.txt` (`Using Tessera`, `How Tessera Works`,
 *   `Optional`), `/llms-full.txt` (external user core only), and
 *   `/llms-explain.txt` (algorithm overview and chapters 1–7).
 *
 * Caller contract:
 * - `docs/.vitepress/config.ts` calls `discoverSitePages` once for `srcExclude`
 *   and `rewrites`, `resolveRepositoryLink` inside its markdown-it plugin,
 *   `buildPageHeadMeta` inside `transformPageData`, and `exportLlmsArtifacts`
 *   inside `buildEnd`.
 * - Benchmark comparison rendering is injected via `ComparisonRenderer` (backed by
 *   `comparisonMarkdown.ts`) so this module does not duplicate `comparison.json`
 *   fetching, schema validation, or table formatting, and imports
 *   `COMPARISON_DATA_URL` directly from `comparisonMarkdown.ts`.
 *
 * Failure semantics:
 * - Unresolved or out-of-repo link targets return `{ kind: "passthrough" }` so
 *   VitePress's dead-link check reports broken links during build rather than
 *   silently rewriting them.
 * - When no comparison snapshot or renderer is available, `<ImplementationComparison />`
 *   emits a direct link to the published `comparison.json` instead of failing.
 * - Any newly added page not explicitly listed in a tier defaults to `## Optional`
 *   in `llms.txt` so every emitted `.md` file is indexed without polluting the
 *   external-user tiers.
 */

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join, relative, resolve, sep } from "node:path";
import type { HeadConfig } from "vitepress";
import { COMPARISON_DATA_URL, type ComparisonView } from "./comparisonMarkdown.ts";

export const REPOSITORY_URL = "https://github.com/Yikai-Liao/tessera-order";
export const DEFAULT_SITE_ORIGIN = "https://yikai-liao.github.io";
export const DEFAULT_DOCS_BASE = "/tessera-order/";
export const HISTORY_DATA_URL = `${REPOSITORY_URL}/tree/benchmark-data/tessera`;

export const SHARED_PAGES: ReadonlySet<string> = new Set([
  "conformance/semantics.md",
  "ROADMAP.md",
  "RELEASING.md",
  "CONTRIBUTING.md",
]);

// The task index is for repository readers; the site has its own home page.
export const NOT_PAGES: ReadonlySet<string> = new Set(["docs/README.md"]);

export type { ComparisonView };
export type ComparisonRenderer = (view: ComparisonView) => string | null | undefined;

export interface SitePageEntry {
  sourcePath: string;
  sitePath: string;
}

export interface SiteCatalog {
  allMarkdown: string[];
  pages: SitePageEntry[];
  sitePageSet: Set<string>;
  rewrites: Record<string, string>;
  srcExclude: string[];
}

export type ResolvedRepositoryLink =
  | { kind: "page"; sitePath: string; anchor: string }
  | { kind: "repository"; repoPath: string; entryKind: "blob" | "tree"; anchor: string }
  | { kind: "passthrough" };

export interface HomeFeature {
  title: string;
  details: string;
  link?: string;
}

export interface ParsedFrontmatter {
  title?: string;
  description?: string;
  layout?: string;
  hero?: {
    name?: string;
    text?: string;
    tagline?: string;
  };
  features?: HomeFeature[];
}

export interface RenderedPage {
  sourcePath: string;
  sitePath: string;
  title: string;
  description: string;
  markdown: string;
}

export interface ExportLlmsOptions {
  repoRoot: string;
  outDir: string;
  siteUrl: string;
  catalog?: SiteCatalog;
  renderComparison?: ComparisonRenderer;
}

export interface ExportLlmsResult {
  sitePaths: string[];
  llmsTxt: string;
  llmsFullTxt: string;
  llmsExplainTxt: string;
}

interface TierSpec {
  sitePath: string;
  title?: string;
  summary: string;
}

const USING_TESSERA_SPECS: readonly TierSpec[] = [
  {
    sitePath: "index.md",
    title: "Tessera overview",
    summary: "Quick example, fit criteria, and benchmark overview for O(√n) stable radix sorting.",
  },
  {
    sitePath: "guide/getting-started.md",
    summary: "Installation, operations, examples, Rayon parallelism, and version notes.",
  },
  {
    sitePath: "conformance/semantics.md",
    summary:
      "Formal contracts for key encoding, ordering, stability, panic recovery, and extra memory bounds.",
  },
  {
    sitePath: "benchmarks/index.md",
    summary: "Performance summary across input shapes and main-branch benchmark history.",
  },
  {
    sitePath: "benchmarks/implementations.md",
    summary: "Speed and extra-memory comparisons against Rust and C++ sorting libraries.",
  },
];

const HOW_IT_WORKS_SPECS: readonly TierSpec[] = [
  {
    sitePath: "explain/index.md",
    summary: "High-level pipeline overview and chapter reading order.",
  },
  {
    sitePath: "explain/radix-sort.md",
    summary: "Counting passes, LSD versus MSD, and how Tessera combines them.",
  },
  {
    sitePath: "explain/keys.md",
    summary: "Encoding signed integers, floats, tuples, and Reverse as unsigned integers.",
  },
  {
    sitePath: "explain/routing.md",
    summary: "Skipping radix passes on sorted, reversed, nearly sorted, constant, and skewed data.",
  },
  {
    sitePath: "explain/block-partition.md",
    summary: "In-place O(√n) stable block partitioning, parallel form, and block-size derivation.",
  },
  {
    sitePath: "explain/tiles.md",
    summary:
      "Finishing cache-sized tiles with insertion sort, tile MSD, tile LSD, and digit plans.",
  },
  {
    sitePath: "explain/safety.md",
    summary: "Single-owner record movement, stale copies, inconsistent keys, and panic recovery.",
  },
  {
    sitePath: "explain/operations.md",
    summary: "How argsort, single-rank selection, and cached-key sorting reuse the engine.",
  },
];

const OPTIONAL_SPECS: readonly TierSpec[] = [
  {
    sitePath: "design/engine.md",
    summary: "Internal engine module boundaries, ownership invariants, and parallel completion.",
  },
  {
    sitePath: "design/evidence.md",
    summary: "Internal performance experiments, open measured losses, and rejected directions.",
  },
  {
    sitePath: "design/benchmark-tooling.md",
    summary: "Benchmark CLI module ownership and change boundaries.",
  },
  {
    sitePath: "ROADMAP.md",
    summary: "Unfinished work and acceptance conditions.",
  },
  {
    sitePath: "CONTRIBUTING.md",
    summary: "Contributor workflow, local checks, and PR acceptance rules.",
  },
  {
    sitePath: "RELEASING.md",
    summary: "Independent package versioning and release procedure.",
  },
  {
    sitePath: "development/benchmarking.md",
    summary: "Local benchmark commands, input patterns, and statistics.",
  },
  {
    sitePath: "development/benchmark-figures.md",
    summary: "Shared visual grammar for benchmark charts and reports.",
  },
  {
    sitePath: "development/continuous-benchmarks.md",
    summary: "Hosted PR comparison and main-branch tracking pipeline.",
  },
  {
    sitePath: "development/ci.md",
    summary: "Continuous integration lanes, platforms, and routing.",
  },
  {
    sitePath: "development/docs-site.md",
    summary: "VitePress site layout, commands, and page authoring rules.",
  },
  {
    sitePath: "design/operation-extensions-proposal.md",
    summary:
      "Design record for the 0.2 operation surface and API scope rules for future extensions.",
  },
  {
    sitePath: "design/python-binding.md",
    summary: "Unimplemented Python binding design.",
  },
  {
    sitePath: "design/ci-scaling-proposal.md",
    summary: "Proposal for CI reuse, operation benchmark selection, and caching.",
  },
  {
    sitePath: "design/comparison-model-proposal.md",
    summary: "Proposal for comparing operations beyond full sorting.",
  },
];

export const LLMS_FULL_PAGES: readonly string[] = [
  "index.md",
  "guide/getting-started.md",
  "conformance/semantics.md",
  "benchmarks/implementations.md",
  "benchmarks/index.md",
  "explain/index.md",
];

export const LLMS_EXPLAIN_PAGES: readonly string[] = HOW_IT_WORKS_SPECS.map((s) => s.sitePath);

export function isPage(repoPath: string): boolean {
  if (!repoPath.endsWith(".md") || NOT_PAGES.has(repoPath)) return false;
  return (
    SHARED_PAGES.has(repoPath) ||
    (repoPath.startsWith("docs/") && !repoPath.startsWith("docs/.vitepress/"))
  );
}

/** Where a source file is served: `docs/` is the site root, `README.md` is a directory index. */
export function sitePath(repoPath: string): string {
  const served = repoPath.startsWith("docs/") ? repoPath.slice("docs/".length) : repoPath;
  return served.replace(/(^|\/)README\.md$/, "$1index.md");
}

/** Canonical public base URL (always ending with `/`), respecting an explicit `DOCS_BASE`. */
export function canonicalSiteUrl(docsBase?: string): string {
  const raw = docsBase === undefined ? DEFAULT_DOCS_BASE : docsBase;
  const trimmed = raw.trim();
  if (/^https?:\/\//i.test(trimmed)) {
    return trimmed.endsWith("/") ? trimmed : `${trimmed}/`;
  }
  const withLeading = trimmed.startsWith("/") ? trimmed : `/${trimmed}`;
  const normalized = withLeading.endsWith("/") ? withLeading : `${withLeading}/`;
  return `${DEFAULT_SITE_ORIGIN}${normalized}`;
}

export function pageCanonicalUrl(relativeSitePath: string, siteUrl: string): string {
  const clean = relativeSitePath.replace(/(^|\/)index\.md$/, "$1").replace(/\.md$/, "");
  return `${siteUrl}${clean}`;
}

export function pageMarkdownUrl(relativeSitePath: string, siteUrl: string): string {
  return `${siteUrl}${relativeSitePath}`;
}

function collectMarkdownFiles(repoRoot: string, dir: string, out: string[]): void {
  for (const entry of readdirSync(dir)) {
    if (entry === "node_modules" || entry === "target" || entry.startsWith(".git")) continue;
    const full = join(dir, entry);
    const stat = statSync(full);
    if (stat.isDirectory()) {
      if (entry === ".vitepress") continue;
      collectMarkdownFiles(repoRoot, full, out);
    } else if (entry.endsWith(".md")) {
      out.push(relative(repoRoot, full).split(sep).join("/"));
    }
  }
}

export function discoverSitePages(repoRoot: string): SiteCatalog {
  const allMarkdown: string[] = [];
  collectMarkdownFiles(repoRoot, repoRoot, allMarkdown);
  allMarkdown.sort();

  const pages: SitePageEntry[] = [];
  const sitePageSet = new Set<string>();
  const rewrites: Record<string, string> = {};
  const srcExclude: string[] = [];

  for (const repoPath of allMarkdown) {
    if (isPage(repoPath)) {
      const served = sitePath(repoPath);
      pages.push({ sourcePath: repoPath, sitePath: served });
      sitePageSet.add(served);
      if (served !== repoPath) rewrites[repoPath] = served;
    } else {
      srcExclude.push(repoPath);
    }
  }

  return { allMarkdown, pages, sitePageSet, rewrites, srcExclude };
}

/**
 * Resolves a relative link in a Markdown document against the repository root.
 * External URLs, fragment-only links, site-root paths, and missing targets
 * return `{ kind: "passthrough" }`.
 */
export function resolveRepositoryLink(
  repoRoot: string,
  sourceAbsPath: string,
  href: string,
): ResolvedRepositoryLink {
  if (!href || /^[a-z][a-z0-9+.-]*:/i.test(href) || href.startsWith("#") || href.startsWith("/")) {
    return { kind: "passthrough" };
  }
  const hashIndex = href.indexOf("#");
  const rawPath = hashIndex === -1 ? href : href.slice(0, hashIndex);
  const anchor = hashIndex === -1 ? "" : href.slice(hashIndex);
  if (!rawPath) return { kind: "passthrough" };

  let decoded: string;
  try {
    decoded = decodeURIComponent(rawPath);
  } catch {
    return { kind: "passthrough" };
  }

  const target = resolve(dirname(sourceAbsPath), decoded);
  const rel = relative(repoRoot, target).split(sep).join("/");
  if (rel.startsWith("..") || !existsSync(target)) {
    return { kind: "passthrough" };
  }
  if (isPage(rel)) {
    return { kind: "page", sitePath: sitePath(rel), anchor };
  }
  const entryKind = statSync(target).isDirectory() ? "tree" : "blob";
  return { kind: "repository", repoPath: rel, entryKind, anchor };
}

export function formatRepositoryUrl(
  resolved: Extract<ResolvedRepositoryLink, { kind: "repository" }>,
): string {
  return `${REPOSITORY_URL}/${resolved.entryKind}/main/${resolved.repoPath}${resolved.anchor}`;
}

export function resolveSiteRootLink(
  href: string,
  sitePages: ReadonlySet<string>,
): { sitePath: string; anchor: string } | null {
  if (!href.startsWith("/") || href.startsWith("//")) return null;
  const hashIndex = href.indexOf("#");
  const rawPath = (hashIndex === -1 ? href : href.slice(0, hashIndex)).slice(1);
  const anchor = hashIndex === -1 ? "" : href.slice(hashIndex);

  const candidates =
    rawPath === ""
      ? ["index.md"]
      : rawPath.endsWith("/")
        ? [`${rawPath}index.md`]
        : rawPath.endsWith(".md")
          ? [rawPath]
          : [`${rawPath}.md`, `${rawPath}/index.md`];

  for (const candidate of candidates) {
    if (sitePages.has(candidate)) {
      return { sitePath: candidate, anchor };
    }
  }
  return null;
}

function unquoteYaml(value: string): string {
  const trimmed = value.trim();
  if (
    (trimmed.startsWith('"') && trimmed.endsWith('"')) ||
    (trimmed.startsWith("'") && trimmed.endsWith("'"))
  ) {
    return trimmed.slice(1, -1);
  }
  return trimmed;
}

/** Parses the subset of YAML frontmatter used by Tessera pages (scalars, hero, features). */
export function parseFrontmatter(raw: string): { frontmatter: ParsedFrontmatter; body: string } {
  const match = raw.match(/^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/);
  if (!match) return { frontmatter: {}, body: raw };

  const frontmatter: ParsedFrontmatter = {};
  const lines = match[1].split(/\r?\n/);
  let section: "hero" | "features" | null = null;
  let currentFeature: Partial<HomeFeature> | null = null;

  const commitFeature = () => {
    if (currentFeature?.title && currentFeature?.details) {
      frontmatter.features ??= [];
      frontmatter.features.push({
        title: currentFeature.title,
        details: currentFeature.details,
        ...(currentFeature.link ? { link: currentFeature.link } : {}),
      });
    }
    currentFeature = null;
  };

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) continue;

    const indent = line.length - line.trimStart().length;
    if (indent === 0) {
      commitFeature();
      const kv = trimmed.match(/^([A-Za-z0-9_-]+):\s*(.*)$/);
      if (!kv) continue;
      const [, key, value] = kv;
      if (key === "hero" && !value) {
        section = "hero";
        frontmatter.hero ??= {};
      } else if (key === "features" && !value) {
        section = "features";
      } else {
        section = null;
        if (key === "title") frontmatter.title = unquoteYaml(value);
        else if (key === "description") frontmatter.description = unquoteYaml(value);
        else if (key === "layout") frontmatter.layout = unquoteYaml(value);
      }
      continue;
    }

    if (section === "hero" && indent === 2) {
      const kv = trimmed.match(/^([A-Za-z0-9_-]+):\s*(.+)$/);
      if (!kv) continue;
      const [, key, value] = kv;
      frontmatter.hero ??= {};
      if (key === "name") frontmatter.hero.name = unquoteYaml(value);
      else if (key === "text") frontmatter.hero.text = unquoteYaml(value);
      else if (key === "tagline") frontmatter.hero.tagline = unquoteYaml(value);
    } else if (section === "features") {
      if (trimmed.startsWith("- ")) {
        commitFeature();
        currentFeature = {};
        const kv = trimmed.slice(2).match(/^([A-Za-z0-9_-]+):\s*(.+)$/);
        if (kv) {
          const [, key, value] = kv;
          if (key === "title") currentFeature.title = unquoteYaml(value);
          else if (key === "details") currentFeature.details = unquoteYaml(value);
          else if (key === "link") currentFeature.link = unquoteYaml(value);
        }
      } else if (currentFeature) {
        const kv = trimmed.match(/^([A-Za-z0-9_-]+):\s*(.+)$/);
        if (kv) {
          const [, key, value] = kv;
          if (key === "title") currentFeature.title = unquoteYaml(value);
          else if (key === "details") currentFeature.details = unquoteYaml(value);
          else if (key === "link") currentFeature.link = unquoteYaml(value);
        }
      }
    }
  }
  commitFeature();

  return { frontmatter, body: raw.slice(match[0].length) };
}

const STANDALONE_VUE_TAG_RE = /^<([A-Z][A-Za-z0-9]*)(?:\s+([^>]*?))?\s*\/?>(?:\s*<\/\1>)?$/;
const SPDX_COMMENT_RE = /^<!--\s*SPDX-License-Identifier:[^>]*-->$/;
const BARE_ANCHOR_RE = /^<a\s+id="[^"]*"\s*>\s*<\/a>$/i;

function stripInlineMarkdown(text: string): string {
  // Shield inline code spans first so generics like `Box<T>` survive HTML stripping
  // while links whose labels contain code (like [`foo`](url)) still unwrap cleanly.
  const codeSpans: string[] = [];
  const shielded = text.replace(/`([^`]+)`/g, (_match, inner: string) => {
    const token = `\uE000CODE_${codeSpans.length}\uE001`;
    codeSpans.push(inner);
    return token;
  });
  const stripped = shielded
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/\*\*([^*]+)\*\*/g, "$1")
    .replace(/__([^_]+)__/g, "$1")
    .replace(/\*([^*]+)\*/g, "$1")
    .replace(/(^|\W)_([^_]+)_(?=\W|$)/g, "$1$2")
    .replace(/<\/?[a-zA-Z][^>]*>/g, "");
  return stripped
    .replace(/\uE000CODE_(\d+)\uE001/g, (_match, index: string) => codeSpans[Number(index)] ?? "")
    .replace(/\s+/g, " ")
    .trim();
}

function truncateDescription(text: string, maxLength = 240): string {
  if (text.length <= maxLength) return text;
  const slice = text.slice(0, maxLength);
  const sentenceEnd = slice.lastIndexOf(". ");
  if (sentenceEnd >= Math.floor(maxLength * 0.5)) {
    return slice.slice(0, sentenceEnd + 1);
  }
  const wordEnd = slice.lastIndexOf(" ");
  return `${(wordEnd > 0 ? slice.slice(0, wordEnd) : slice).trimEnd()}…`;
}

/** Extracts a page's clean title and plain-text description from frontmatter or body prose. */
export function extractPageMetadata(
  raw: string,
  fallbackTitle = "Tessera",
): { title: string; description: string } {
  const { frontmatter, body } = parseFrontmatter(raw);
  const lines = body.split(/\r?\n/);

  let headingTitle = "";
  let firstParagraph = "";
  const paragraphLines: string[] = [];

  let fence: { char: string; length: number } | null = null;
  let inScript = false;
  let prevBlank = true;
  let inIndentedCode = false;

  const flushParagraph = () => {
    if (!firstParagraph && paragraphLines.length > 0) {
      const plain = stripInlineMarkdown(paragraphLines.join(" "));
      // Skip short list lead-ins such as "In the current measurements:".
      if (plain.length > 0 && !(plain.endsWith(":") && plain.length < 60)) {
        firstParagraph = plain;
      }
    }
    paragraphLines.length = 0;
  };

  for (const line of lines) {
    const trimmed = line.trim();

    if (fence) {
      const closeMatch = trimmed.match(/^(`{3,}|~{3,})\s*$/);
      if (closeMatch && closeMatch[1][0] === fence.char && closeMatch[1].length >= fence.length) {
        fence = null;
      }
      prevBlank = false;
      continue;
    }

    if (inScript) {
      if (/<\/script\s*>/i.test(trimmed)) inScript = false;
      prevBlank = false;
      continue;
    }

    const fenceOpen = line.match(/^\s*(`{3,}|~{3,})/);
    if (fenceOpen) {
      flushParagraph();
      fence = { char: fenceOpen[1][0], length: fenceOpen[1].length };
      prevBlank = false;
      inIndentedCode = false;
      continue;
    }

    if (trimmed === "") {
      flushParagraph();
      prevBlank = true;
      inIndentedCode = false;
      continue;
    }

    if ((prevBlank || inIndentedCode) && (line.startsWith("\t") || line.startsWith("    "))) {
      flushParagraph();
      inIndentedCode = true;
      prevBlank = false;
      continue;
    }
    inIndentedCode = false;

    if (/^<script\b/i.test(trimmed)) {
      flushParagraph();
      if (!/<\/script\s*>/i.test(trimmed)) inScript = true;
      prevBlank = false;
      continue;
    }

    if (
      SPDX_COMMENT_RE.test(trimmed) ||
      STANDALONE_VUE_TAG_RE.test(trimmed) ||
      BARE_ANCHOR_RE.test(trimmed)
    ) {
      flushParagraph();
      prevBlank = false;
      continue;
    }

    const h1Match = trimmed.match(/^#\s+(.+)$/);
    if (h1Match) {
      flushParagraph();
      if (!headingTitle) headingTitle = stripInlineMarkdown(h1Match[1]);
      prevBlank = false;
      continue;
    }

    if (
      /^#{1,6}\s+/.test(trimmed) ||
      trimmed.startsWith("|") ||
      /^([-*+]\s+|\d+\.\s+)/.test(trimmed) ||
      trimmed.startsWith(">") ||
      /^[-*_]{3,}$/.test(trimmed) ||
      trimmed.startsWith("$$")
    ) {
      flushParagraph();
      prevBlank = false;
      continue;
    }

    if (!firstParagraph) {
      paragraphLines.push(trimmed);
    }
    prevBlank = false;
  }
  flushParagraph();

  const heroTitle =
    frontmatter.hero?.name && frontmatter.hero?.text
      ? `${frontmatter.hero.name} — ${frontmatter.hero.text}`
      : frontmatter.hero?.name;

  const title = frontmatter.title || headingTitle || heroTitle || fallbackTitle;
  const rawDescription =
    frontmatter.description ||
    frontmatter.hero?.tagline ||
    firstParagraph ||
    "Stable radix ordering with O(√n) auxiliary memory.";

  return { title, description: truncateDescription(rawDescription) };
}

/** Builds per-page description and `<head>` metadata for VitePress `transformPageData`. */
export function buildPageHeadMeta(params: {
  relativePath: string;
  filePath: string;
  title: string;
  description: string;
  frontmatter: Record<string, unknown>;
  srcDir: string;
  siteUrl: string;
}): { description: string; head: HeadConfig[] } {
  const { relativePath, filePath, srcDir, siteUrl } = params;
  let description = params.description?.trim() ?? "";
  let extractedTitle = params.title?.trim() ?? "";

  if ((!description || !extractedTitle) && filePath) {
    const absPath = resolve(srcDir, filePath);
    if (existsSync(absPath)) {
      const meta = extractPageMetadata(readFileSync(absPath, "utf8"), extractedTitle || "Tessera");
      if (!description) description = meta.description;
      if (!extractedTitle) extractedTitle = meta.title;
    }
  }
  if (!description) {
    description = "Stable radix ordering with O(√n) auxiliary memory.";
  }

  const canonicalUrl = pageCanonicalUrl(relativePath, siteUrl);
  const markdownUrl = pageMarkdownUrl(relativePath, siteUrl);
  const isHome = relativePath === "index.md";
  const ogTitle =
    isHome || !extractedTitle || extractedTitle === "Tessera"
      ? extractedTitle || "Tessera"
      : `${extractedTitle} | Tessera`;

  const existingHead = Array.isArray(params.frontmatter.head)
    ? ([...params.frontmatter.head] as HeadConfig[])
    : [];

  const hasTag = (predicate: (entry: HeadConfig) => boolean) => existingHead.some(predicate);
  const pushIfMissing = (entry: HeadConfig, predicate: (existing: HeadConfig) => boolean) => {
    if (!hasTag(predicate)) existingHead.push(entry);
  };

  pushIfMissing(
    ["link", { rel: "canonical", href: canonicalUrl }],
    ([tag, attrs]) => tag === "link" && attrs.rel === "canonical",
  );
  pushIfMissing(
    ["link", { rel: "alternate", type: "text/markdown", href: markdownUrl }],
    ([tag, attrs]) => tag === "link" && attrs.rel === "alternate" && attrs.type === "text/markdown",
  );
  pushIfMissing(
    ["meta", { property: "og:title", content: ogTitle }],
    ([tag, attrs]) => tag === "meta" && attrs.property === "og:title",
  );
  pushIfMissing(
    ["meta", { property: "og:description", content: description }],
    ([tag, attrs]) => tag === "meta" && attrs.property === "og:description",
  );
  pushIfMissing(
    ["meta", { property: "og:url", content: canonicalUrl }],
    ([tag, attrs]) => tag === "meta" && attrs.property === "og:url",
  );
  pushIfMissing(
    ["meta", { property: "og:type", content: isHome ? "website" : "article" }],
    ([tag, attrs]) => tag === "meta" && attrs.property === "og:type",
  );

  return { description, head: existingHead };
}

function rewriteLinkTarget(
  href: string,
  sourceAbsPath: string,
  repoRoot: string,
  siteUrl: string,
  sitePages: ReadonlySet<string>,
): string {
  const siteRoot = resolveSiteRootLink(href, sitePages);
  if (siteRoot) {
    return `${pageMarkdownUrl(siteRoot.sitePath, siteUrl)}${siteRoot.anchor}`;
  }
  const resolved = resolveRepositoryLink(repoRoot, sourceAbsPath, href);
  if (resolved.kind === "page") {
    return `${pageMarkdownUrl(resolved.sitePath, siteUrl)}${resolved.anchor}`;
  }
  if (resolved.kind === "repository") {
    return formatRepositoryUrl(resolved);
  }
  return href;
}

/**
 * Rewrites Markdown links on one non-code line while skipping inline code spans
 * and preserving link titles and balanced parentheses inside destinations.
 */
function rewriteLineLinks(line: string, rewriteHref: (href: string) => string): string {
  // Handle reference-style link definitions: `[id]: destination "optional title"`.
  const refDef = line.match(/^(\s*\[[^\]]+\]:\s*)(<[^>]+>|\S+)(\s*.*)$/);
  if (refDef) {
    const [, prefix, dest, suffix] = refDef;
    const unwrapped = dest.startsWith("<") && dest.endsWith(">") ? dest.slice(1, -1) : dest;
    const rewritten = rewriteHref(unwrapped);
    const wrapped = dest.startsWith("<") && dest.endsWith(">") ? `<${rewritten}>` : rewritten;
    return `${prefix}${wrapped}${suffix}`;
  }

  let out = "";
  let i = 0;

  while (i < line.length) {
    // Skip inline code spans verbatim.
    if (line[i] === "`") {
      let tickEnd = i;
      while (tickEnd < line.length && line[tickEnd] === "`") tickEnd++;
      const ticks = line.slice(i, tickEnd);
      const closeIndex = line.indexOf(ticks, tickEnd);
      if (closeIndex !== -1) {
        out += line.slice(i, closeIndex + ticks.length);
        i = closeIndex + ticks.length;
        continue;
      }
      out += ticks;
      i = tickEnd;
      continue;
    }

    if (line[i] === "\\" && i + 1 < line.length) {
      out += line.slice(i, i + 2);
      i += 2;
      continue;
    }

    if (line[i] === "[") {
      // Scan matching `]` while respecting nested brackets and inline code spans.
      let depth = 1;
      let j = i + 1;
      while (j < line.length && depth > 0) {
        if (line[j] === "\\" && j + 1 < line.length) {
          j += 2;
          continue;
        }
        if (line[j] === "`") {
          let tickEnd = j;
          while (tickEnd < line.length && line[tickEnd] === "`") tickEnd++;
          const ticks = line.slice(j, tickEnd);
          const closeIndex = line.indexOf(ticks, tickEnd);
          if (closeIndex !== -1) {
            j = closeIndex + ticks.length;
            continue;
          }
          j = tickEnd;
          continue;
        }
        if (line[j] === "[") depth++;
        else if (line[j] === "]") depth--;
        j++;
      }

      if (depth === 0 && line[j] === "(") {
        // Scan matching `)` allowing nested parentheses in URLs.
        let parenDepth = 1;
        let k = j + 1;
        while (k < line.length && parenDepth > 0) {
          if (line[k] === "\\" && k + 1 < line.length) {
            k += 2;
            continue;
          }
          if (line[k] === "(") parenDepth++;
          else if (line[k] === ")") parenDepth--;
          k++;
        }

        if (parenDepth === 0) {
          const label = line.slice(i, j); // includes `[...]`
          const inner = line.slice(j + 1, k - 1);
          const destMatch = inner.match(/^(\s*)(<[^>]+>|\S+)([\s\S]*)$/);
          if (destMatch) {
            const [, leading, rawDest, trailing] = destMatch;
            const unwrapped =
              rawDest.startsWith("<") && rawDest.endsWith(">") ? rawDest.slice(1, -1) : rawDest;
            const rewritten = rewriteHref(unwrapped);
            const wrapped =
              rawDest.startsWith("<") && rawDest.endsWith(">") ? `<${rewritten}>` : rewritten;
            out += `${label}(${leading}${wrapped}${trailing})`;
          } else {
            out += line.slice(i, k);
          }
          i = k;
          continue;
        }
      }
    }

    out += line[i];
    i++;
  }

  return out;
}

function expandHomeFrontmatter(
  frontmatter: ParsedFrontmatter,
  siteUrl: string,
  sitePages: ReadonlySet<string>,
): string[] {
  const blocks: string[] = [];
  if (frontmatter.hero) {
    const { name, text, tagline } = frontmatter.hero;
    if (name && text) blocks.push(`# ${name} — ${text}`);
    else if (name) blocks.push(`# ${name}`);
    else if (text) blocks.push(`# ${text}`);
    if (tagline) blocks.push(tagline);
  }
  if (frontmatter.features?.length) {
    const bullets = frontmatter.features.map((feature) => {
      let suffix = "";
      if (feature.link) {
        const resolved = resolveSiteRootLink(feature.link, sitePages);
        const url = resolved
          ? `${pageMarkdownUrl(resolved.sitePath, siteUrl)}${resolved.anchor}`
          : feature.link;
        suffix = ` ([Details](${url}))`;
      }
      return `- **${feature.title}:** ${feature.details}${suffix}`;
    });
    blocks.push(bullets.join("\n"));
  }
  return blocks;
}

function parseComparisonView(attrs: string | undefined): ComparisonView {
  if (!attrs) return "all";
  const match = attrs.match(/\bview\s*=\s*["'](overview|all)["']/);
  return match?.[1] === "overview" ? "overview" : "all";
}

/** Projects a repository Markdown source file into a clean AI-consumable `.md` mirror. */
export function renderPageMarkdown(params: {
  raw: string;
  sourceAbsPath: string;
  repoRoot: string;
  siteUrl: string;
  sitePages: ReadonlySet<string>;
  renderComparison?: ComparisonRenderer;
}): string {
  const { raw, sourceAbsPath, repoRoot, siteUrl, sitePages, renderComparison } = params;
  const { frontmatter, body } = parseFrontmatter(raw);

  const outputLines: string[] = [];
  const homeHeaderBlocks = expandHomeFrontmatter(frontmatter, siteUrl, sitePages);
  if (homeHeaderBlocks.length > 0) {
    outputLines.push(homeHeaderBlocks.join("\n\n"), "");
  }

  const rewriteHref = (href: string) =>
    rewriteLinkTarget(href, sourceAbsPath, repoRoot, siteUrl, sitePages);

  const lines = body.split(/\r?\n/);
  let fence: { char: string; length: number } | null = null;
  let inScript = false;
  let prevBlank = true;
  let inIndentedCode = false;

  for (const line of lines) {
    const trimmed = line.trim();

    if (fence) {
      outputLines.push(line);
      const closeMatch = trimmed.match(/^(`{3,}|~{3,})\s*$/);
      if (closeMatch && closeMatch[1][0] === fence.char && closeMatch[1].length >= fence.length) {
        fence = null;
      }
      prevBlank = false;
      continue;
    }

    if (inScript) {
      if (/<\/script\s*>/i.test(trimmed)) inScript = false;
      continue;
    }

    const fenceOpen = line.match(/^\s*(`{3,}|~{3,})/);
    if (fenceOpen) {
      fence = { char: fenceOpen[1][0], length: fenceOpen[1].length };
      outputLines.push(line);
      prevBlank = false;
      inIndentedCode = false;
      continue;
    }

    if (trimmed === "") {
      outputLines.push("");
      prevBlank = true;
      inIndentedCode = false;
      continue;
    }

    if ((prevBlank || inIndentedCode) && (line.startsWith("\t") || line.startsWith("    "))) {
      inIndentedCode = true;
      outputLines.push(line);
      prevBlank = false;
      continue;
    }
    inIndentedCode = false;

    if (/^<script\b/i.test(trimmed)) {
      if (!/<\/script\s*>/i.test(trimmed)) inScript = true;
      continue;
    }

    if (SPDX_COMMENT_RE.test(trimmed)) {
      continue;
    }

    const vueMatch = trimmed.match(STANDALONE_VUE_TAG_RE);
    if (vueMatch) {
      const [, tagName, attrs] = vueMatch;
      if (tagName === "ImplementationComparison") {
        const view = parseComparisonView(attrs);
        const rendered = renderComparison?.(view)?.trim();
        if (rendered) {
          outputLines.push(rendered);
        } else {
          outputLines.push(
            `Live benchmark comparison data: [comparison.json](${COMPARISON_DATA_URL}).`,
          );
        }
      } else if (tagName === "PerformanceHistory") {
        outputLines.push(
          `Interactive main-branch benchmark history data: [benchmark-data/tessera](${HISTORY_DATA_URL}).`,
        );
      }
      prevBlank = false;
      continue;
    }

    outputLines.push(rewriteLineLinks(line, rewriteHref));
    prevBlank = false;
  }

  return `${outputLines
    .join("\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim()}\n`;
}

function formatTierEntry(spec: TierSpec, page: RenderedPage, siteUrl: string): string {
  const title = spec.title ?? page.title;
  const url = pageMarkdownUrl(page.sitePath, siteUrl);
  return `- [${title}](${url}): ${spec.summary}`;
}

/** Builds the audience-tiered `/llms.txt` index following llmstxt.org conventions. */
export function buildLlmsIndex(pages: ReadonlyMap<string, RenderedPage>, siteUrl: string): string {
  const included = new Set<string>();
  const lines: string[] = [
    "# Tessera",
    "",
    "> Stable radix ordering library for Rust with O(√n) auxiliary memory, preserving input order for equal keys without an input-sized buffer.",
    "",
    `For single-fetch AI consumption, use [llms-full.txt](${siteUrl}llms-full.txt) (user guide, semantics, benchmarks, and overview) or [llms-explain.txt](${siteUrl}llms-explain.txt) (complete algorithm deep-dive, chapters 1–7). Every documentation page is also published as clean Markdown at its \`.md\` URL below.`,
    "",
    "## Using Tessera",
    "",
  ];

  for (const spec of USING_TESSERA_SPECS) {
    const page = pages.get(spec.sitePath);
    if (!page) continue;
    included.add(spec.sitePath);
    lines.push(formatTierEntry(spec, page, siteUrl));
  }

  lines.push("", "## How Tessera Works", "");
  for (const spec of HOW_IT_WORKS_SPECS) {
    const page = pages.get(spec.sitePath);
    if (!page) continue;
    included.add(spec.sitePath);
    lines.push(formatTierEntry(spec, page, siteUrl));
  }

  lines.push(
    "",
    "## Optional",
    "",
    "Internal contributor documentation, benchmark engineering notes, and unimplemented design proposals. External integration agents can skip this section.",
    "",
  );
  for (const spec of OPTIONAL_SPECS) {
    const page = pages.get(spec.sitePath);
    if (!page) continue;
    included.add(spec.sitePath);
    lines.push(formatTierEntry(spec, page, siteUrl));
  }

  // Place any newly added page not explicitly listed above under Optional so no page is orphaned.
  const remaining = [...pages.values()]
    .filter((page) => !included.has(page.sitePath))
    .sort((a, b) => a.sitePath.localeCompare(b.sitePath));
  for (const page of remaining) {
    lines.push(
      `- [${page.title}](${pageMarkdownUrl(page.sitePath, siteUrl)}): ${page.description}`,
    );
  }

  return `${lines.join("\n")}\n`;
}

/** Builds a concatenated single-fetch Markdown bundle with clear source separators. */
export function buildLlmsBundle(params: {
  title: string;
  summary: string;
  sitePaths: readonly string[];
  pages: ReadonlyMap<string, RenderedPage>;
  siteUrl: string;
}): string {
  const { title, summary, sitePaths, pages, siteUrl } = params;
  const sections: string[] = [`# ${title}\n\n> ${summary}`];

  for (const path of sitePaths) {
    const page = pages.get(path);
    if (!page) continue;
    const url = pageMarkdownUrl(page.sitePath, siteUrl);
    sections.push(`---\n\nSource: ${url}\n\n${page.markdown.trim()}`);
  }

  return `${sections.join("\n\n")}\n`;
}

/** Emits per-page `.md` mirrors, `llms.txt`, `llms-full.txt`, and `llms-explain.txt` into `outDir`. */
export async function exportLlmsArtifacts(options: ExportLlmsOptions): Promise<ExportLlmsResult> {
  const { repoRoot, outDir, siteUrl, renderComparison } = options;
  const catalog = options.catalog ?? discoverSitePages(repoRoot);
  const renderedPages = new Map<string, RenderedPage>();

  await Promise.all(
    catalog.pages.map(async (entry) => {
      const sourceAbsPath = resolve(repoRoot, entry.sourcePath);
      const raw = await readFile(sourceAbsPath, "utf8");
      const { title, description } = extractPageMetadata(raw, entry.sitePath);
      const markdown = renderPageMarkdown({
        raw,
        sourceAbsPath,
        repoRoot,
        siteUrl,
        sitePages: catalog.sitePageSet,
        renderComparison,
      });
      renderedPages.set(entry.sitePath, {
        sourcePath: entry.sourcePath,
        sitePath: entry.sitePath,
        title,
        description,
        markdown,
      });

      const destPath = join(outDir, entry.sitePath);
      await mkdir(dirname(destPath), { recursive: true });
      await writeFile(destPath, markdown, "utf8");
    }),
  );

  const llmsTxt = buildLlmsIndex(renderedPages, siteUrl);
  const llmsFullTxt = buildLlmsBundle({
    title: "Tessera Documentation (User Core)",
    summary:
      "Single-fetch user bundle for Tessera: overview, getting started, formal ordering semantics, implementation comparison, performance overview, and algorithm summary. Excludes internal contributor notes and unimplemented proposals.",
    sitePaths: LLMS_FULL_PAGES,
    pages: renderedPages,
    siteUrl,
  });
  const llmsExplainTxt = buildLlmsBundle({
    title: "Tessera Algorithm Deep Dive",
    summary:
      "Complete algorithm explanation for Tessera: overview and chapters 1–7 in reading order, preserving LaTeX formulas.",
    sitePaths: LLMS_EXPLAIN_PAGES,
    pages: renderedPages,
    siteUrl,
  });

  await mkdir(outDir, { recursive: true });
  await Promise.all([
    writeFile(join(outDir, "llms.txt"), llmsTxt, "utf8"),
    writeFile(join(outDir, "llms-full.txt"), llmsFullTxt, "utf8"),
    writeFile(join(outDir, "llms-explain.txt"), llmsExplainTxt, "utf8"),
  ]);

  return {
    sitePaths: catalog.pages.map((p) => p.sitePath),
    llmsTxt,
    llmsFullTxt,
    llmsExplainTxt,
  };
}
