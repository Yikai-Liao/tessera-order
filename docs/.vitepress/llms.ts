// SPDX-License-Identifier: BSD-2-Clause

/**
 * Single authority for Tessera's site topology, repository link resolution,
 * per-page metadata, and build-time `.md` / `llms*.txt` exports. Every page's
 * title and summary derive directly from its own Markdown source (no separate
 * page-summary table), and sections group automatically by path and chapter.
 */

import { existsSync, readFileSync, statSync } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join, relative, resolve, sep } from "node:path";
import { parseHTML } from "linkedom";
import { createLogger } from "vite";
import { createMarkdownRenderer, type HeadConfig, type PageData, resolvePages } from "vitepress";
import { COMPARISON_DATA_URL, type ComparisonView, markdownTable } from "./comparisonMarkdown.ts";

import { scenarios } from "./theme/sim/pipeline.ts";

export const REPOSITORY_URL = "https://github.com/Yikai-Liao/tessera-order";
export const HISTORY_DATA_URL = `${REPOSITORY_URL}/tree/benchmark-data/tessera`;
const DEFAULT_ORIGIN = "https://yikai-liao.github.io";
const DEFAULT_DESC = "Stable radix ordering with O(√n) auxiliary memory.";

const SHARED_PAGES = new Set([
  "conformance/semantics.md",
  "ROADMAP.md",
  "RELEASING.md",
  "CONTRIBUTING.md",
]);
const NOT_PAGES = new Set(["docs/README.md"]);

const VUE_TAG_RE = /^<([A-Z][A-Za-z0-9]*)(?:\s+([^>]*?))?\s*\/?>(?:\s*<\/\1>)?$/;
const NOISE_LINE_RE = /^(?:<!--\s*SPDX-License-Identifier:[^>]*-->|<\/?div\b[^>]*>)$/i;

const isPage = (repoPath: string): boolean =>
  repoPath.endsWith(".md") &&
  !NOT_PAGES.has(repoPath) &&
  (SHARED_PAGES.has(repoPath) ||
    (repoPath.startsWith("docs/") && !repoPath.startsWith("docs/.vitepress/")));

const sitePath = (repoPath: string): string =>
  (repoPath.startsWith("docs/") ? repoPath.slice(5) : repoPath).replace(
    /(^|\/)README\.md$/,
    "$1index.md",
  );

export function canonicalSiteUrl(docsBase = "/tessera-order/"): string {
  const t = docsBase.trim();
  if (/^https?:\/\//i.test(t)) return t.endsWith("/") ? t : `${t}/`;
  const path = `${`${t.startsWith("/") ? "" : "/"}${t}`.replace(/\/+$/, "")}/`;
  return `${DEFAULT_ORIGIN}${path === "//" ? "/" : path}`;
}

/** Parse the single-owner chapter sequence from `docs/explain/index.md#reading-order`. */
export async function explainReadingOrder(raw: string): Promise<string[]> {
  const start = raw.search(/^##\s+Reading order\b/m);
  const tail = start >= 0 ? raw.slice(start).replace(/^##\s+Reading order\b[^\n]*\n?/, "") : "";
  const nextHeading = tail.search(/^##\s/m);
  const section = nextHeading >= 0 ? tail.slice(0, nextHeading) : tail;
  renderer ??= createMarkdownRenderer(process.cwd());
  const md = await renderer;
  const { document } = parseHTML(md.render(section, {}));
  const seen = new Set<string>(["explain/index.md"]);
  const order = ["explain/index.md"];
  // Use VitePress's rendered table first column (or list item lead links) so optional table syntax
  // such as omitted leading pipes works while links in description columns never reorder chapters.
  const tableLinks = [...document.querySelectorAll("tbody tr td:first-child a[href]")];
  const entries = tableLinks.length
    ? tableLinks
    : [...document.querySelectorAll("li")].flatMap((li) => {
        const first = li.querySelector("a[href]");
        return first ? [first] : [];
      });
  for (const link of entries) {
    const rawHref = link.getAttribute("href")?.split(/[?#]/, 1)[0] ?? "";
    if (!rawHref || /^[a-z][a-z0-9+.-]*:|\.\.|^\//i.test(rawHref)) continue;
    const stem = rawHref.replace(/^\.\//, "").replace(/\.(?:md|html)$/i, "");
    if (!stem) continue;
    const normalized = `explain/${stem}.md`;
    if (!seen.has(normalized)) {
      seen.add(normalized);
      order.push(normalized);
    }
  }
  return order;
}

export async function discoverSitePages(repoRoot: string) {
  // Use VitePress's discovery and exclusions for both HTML and Markdown exports.
  const { pages: allMarkdown } = await resolvePages(
    repoRoot,
    { srcExclude: ["target/**", ".*/**"] },
    createLogger(),
  );
  let explainOrder: string[] = ["explain/index.md"];
  const pages = await Promise.all(
    allMarkdown.filter(isPage).map(async (sourcePath) => {
      const raw = await readFile(resolve(repoRoot, sourcePath), "utf8");
      const path = sitePath(sourcePath);
      if (path === "explain/index.md") explainOrder = await explainReadingOrder(raw);
      return { sourcePath, sitePath: path, ...(await extractPageMetadata(raw)) };
    }),
  );
  return {
    pages,
    explainOrder,
    sitePageSet: new Set(pages.map((p) => p.sitePath)),
    rewrites: Object.fromEntries(pages.map((p) => [p.sourcePath, p.sitePath])),
    srcExclude: ["target/**", ".*/**", ...allMarkdown.filter((path) => !isPage(path))],
  };
}

/** Resolve existing repository targets to site paths or GitHub; leave missing targets to VitePress. */
export function resolveRepositoryLink(
  repoRoot: string,
  sourceAbsPath: string,
  href: string,
): string {
  if (!href || /^[a-z][a-z0-9+.-]*:/i.test(href) || href.startsWith("#") || href.startsWith("/"))
    return href;
  const [rawPath, hash] = href.split("#", 2);
  const anchor = hash === undefined ? "" : `#${hash}`;
  let decoded: string;
  try {
    decoded = decodeURIComponent(rawPath);
  } catch {
    return href;
  }
  const target = resolve(dirname(sourceAbsPath), decoded);
  const rel = relative(repoRoot, target).split(sep).join("/");
  if (rel.startsWith("..") || !existsSync(target)) return href;
  if (isPage(rel)) return `/${sitePath(rel)}${anchor}`;
  return `${REPOSITORY_URL}/${statSync(target).isDirectory() ? "tree" : "blob"}/main/${rel}${anchor}`;
}

let renderer: ReturnType<typeof createMarkdownRenderer> | undefined;

/** VitePress owns frontmatter, heading and paragraph syntax; keep the raw body for mirrors. */
async function parsePage(raw: string) {
  renderer ??= createMarkdownRenderer(process.cwd());
  const md = await renderer;
  const env: {
    content?: string;
    title?: string;
    frontmatter?: {
      title?: string;
      description?: string;
      hero?: { name?: string; text?: string; tagline?: string };
    };
  } = {};
  const { document } = parseHTML(md.render(raw, env));
  const fm = env.frontmatter ?? {};
  const hero = [fm.hero?.name, fm.hero?.text].filter(Boolean).join(" — ");
  const paragraph = [...document.querySelectorAll("p")].find(
    (p) =>
      !p.closest("li, blockquote, table") &&
      !(p.textContent!.endsWith(":") && p.textContent!.length < 60),
  );
  const description = String(
    fm.description || fm.hero?.tagline || paragraph?.textContent || DEFAULT_DESC,
  )
    .replace(/\s+/g, " ")
    .trim();
  return {
    md,
    fm,
    body: env.content ?? raw,
    title: String(fm.title || env.title || hero || "Tessera"),
    description:
      description.length <= 240
        ? description
        : `${description.slice(0, 240).replace(/\s+\S*$/, "")}…`,
  };
}

export async function extractPageMetadata(raw: string) {
  const { title, description } = await parsePage(raw);
  return { title, description };
}

export async function addPageMetadata(page: PageData, repoRoot: string, siteUrl: string) {
  const { title, description } = await extractPageMetadata(
    readFileSync(resolve(repoRoot, page.filePath), "utf8"),
  );
  const canonical = `${siteUrl}${page.relativePath.replace(/(^|\/)index\.md$/, "$1").replace(/\.md$/, "")}`;
  const isHome = page.relativePath === "index.md";
  const ogTitle =
    isHome || !title || title === "Tessera" ? title || "Tessera" : `${title} | Tessera`;
  const head = Array.isArray(page.frontmatter.head)
    ? ([...page.frontmatter.head] as HeadConfig[])
    : [];
  const add = (tag: string, attrs: Record<string, string>, matchKey: string) => {
    if (!head.some(([t, a]) => t === tag && a[matchKey] === attrs[matchKey]))
      head.push([tag, attrs]);
  };
  add("link", { rel: "canonical", href: canonical }, "rel");
  add(
    "link",
    { rel: "alternate", type: "text/markdown", href: `${siteUrl}${page.relativePath}` },
    "type",
  );
  add("meta", { property: "og:title", content: ogTitle }, "property");
  add("meta", { property: "og:description", content: description }, "property");
  add("meta", { property: "og:url", content: canonical }, "property");
  add("meta", { property: "og:type", content: isHome ? "website" : "article" }, "property");
  page.description = description;
  page.frontmatter.head = head;
}

export async function renderPageMarkdown(p: {
  raw: string;
  sourceAbsPath: string;
  repoRoot: string;
  siteUrl: string;
  sitePages: ReadonlySet<string>;
  renderComparison?: (view: ComparisonView) => string;
}): Promise<string> {
  const { md, fm, body } = await parsePage(p.raw);
  const out: string[] = [];
  if (fm.hero) {
    out.push(
      `# ${[fm.hero.name, fm.hero.text].filter(Boolean).join(" — ")}`,
      "",
      fm.hero.tagline ?? "",
      "",
    );
  }

  const rewriteHref = (href: string) => {
    const resolved = resolveRepositoryLink(p.repoRoot, p.sourceAbsPath, href);
    if (!resolved.startsWith("/") || resolved.startsWith("//")) return resolved;
    const [path, hash] = resolved.slice(1).split("#", 2);
    const candidates = !path
      ? ["index.md"]
      : path.endsWith("/")
        ? [`${path}index.md`]
        : path.endsWith(".md")
          ? [path]
          : [`${path}.md`, `${path}/index.md`];
    const hit = candidates.find((c) => p.sitePages.has(c));
    return hit ? `${p.siteUrl}${hit}${hash === undefined ? "" : `#${hash}`}` : resolved;
  };
  const env = {};
  const tokens = md.parse(body, env);
  const lines = body.split("\n");
  const protectedLines = new Set<number>();
  for (const token of tokens) {
    if (
      !token.map ||
      !["fence", "code_block", "math_block", "html_block", "html_inline"].includes(token.type)
    )
      continue;
    for (let i = token.map[0]; i < token.map[1]; i++) protectedLines.add(i);
  }
  // Repository prose uses one-line paragraphs. Parse destinations on those lines,
  // including reference definitions, and patch only their source spans. This keeps
  // code, inline examples and raw LaTeX intact without a Markdown serializer.
  const parseDestination = md.helpers.parseLinkDestination;
  try {
    for (let i = 0; i < lines.length; i++) {
      if (protectedLines.has(i)) continue;
      const line = lines[i];
      const edits = new Map<number, { end: number; text: string }>();
      md.helpers.parseLinkDestination = (source, start, end) => {
        const result = parseDestination(source, start, end);
        const offset = line.indexOf(source.trimEnd());
        if (result.ok && offset >= 0) {
          const href = rewriteHref(result.str);
          if (href !== result.str)
            edits.set(offset + start, {
              end: offset + result.pos,
              text: source[start] === "<" ? `<${href}>` : href,
            });
        }
        return result;
      };
      md.parseInline(line, env);
      md.block.parse(line, md, {}, []);
      for (const [start, edit] of [...edits].sort(([a], [b]) => b - a)) {
        lines[i] = lines[i].slice(0, start) + edit.text + lines[i].slice(edit.end);
      }
    }
  } finally {
    md.helpers.parseLinkDestination = parseDestination;
  }
  for (const token of tokens) {
    if (!token.map || !["html_block", "html_inline"].includes(token.type)) continue;
    const [start, end] = token.map;
    const html = token.content.replace(/<script\b[^>]*>[\s\S]*?<\/script\s*>/gi, (script) =>
      script.replace(/[^\n]/g, ""),
    );
    const htmlLines = html.split("\n");
    for (let i = start; i < end; i++) {
      const tag = (htmlLines[i - start] ?? "").trim();
      if (!tag || NOISE_LINE_RE.test(tag)) {
        lines[i] = "";
        continue;
      }
      const vue = tag.match(VUE_TAG_RE);
      if (!vue) continue;
      let replacement = "";
      if (vue[1] === "ImplementationComparison") {
        const view = (vue[2]?.match(/\bview\s*=\s*["'](headlines|overview|all)["']/)?.[1] ??
          "all") as ComparisonView;
        replacement =
          p.renderComparison?.(view)?.trim() ||
          (view === "headlines"
            ? ""
            : `Live benchmark comparison data: [comparison.json](${COMPARISON_DATA_URL}).`);
      } else if (vue[1] === "PipelineMap") {
        replacement = markdownTable(
          ["Workload", "Runs", "Skips", "Extra memory (vs LSD buffer)"],
          scenarios.map((s) => [s.label, s.runs, s.skips, s.memory]),
        );
      } else if (vue[1] === "PerformanceHistory") {
        replacement = `Interactive main-branch benchmark history data: [benchmark-data/tessera](${HISTORY_DATA_URL}).`;
      }
      lines[i] = replacement;
    }
  }
  out.push(...lines);
  return `${out
    .join("\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim()}\n`;
}

type SitePage = Awaited<ReturnType<typeof discoverSitePages>>["pages"][number];

/** Path groups are shared by navigation and AI bundles; explanation order comes from `docs/explain/index.md#reading-order`. */
export function pageSections<P extends SitePage>(
  pages: readonly P[],
  explainOrder: readonly string[] = ["explain/index.md"],
) {
  const explainRank = new Map(explainOrder.map((path, index) => [path, index]));
  const group = (p: SitePage) => {
    const path = p.sitePath;
    if (path === "index.md") return "home";
    if (/^(guide|conformance)\//.test(path)) return "guide";
    if (path.startsWith("benchmarks/")) return "benchmarks";
    if (path.startsWith("explain/")) return "explain";
    if (/proposal|python-binding|ROADMAP/.test(path)) return "proposals";
    return path.startsWith("design/") ? "design" : "development";
  };
  const order = (p: SitePage) => {
    if (p.sitePath.startsWith("explain/")) return explainRank.get(p.sitePath) ?? 999;
    return p.sitePath.endsWith("/index.md") || p.sitePath.startsWith("guide/") ? 0 : 999;
  };
  return [
    ["home", "Overview"],
    ["guide", "User guide"],
    ["benchmarks", "Benchmarks"],
    ["explain", "How Tessera works"],
    ["design", "Engine & Architecture"],
    ["development", "Contributor Guides"],
    ["proposals", "Proposals & Roadmap"],
  ].map(([id, text]) => ({
    id,
    text,
    pages: pages
      .filter((p) => group(p) === id)
      .sort((a, b) =>
        id === "benchmarks"
          ? a.sitePath.localeCompare(b.sitePath)
          : order(a) - order(b) || a.sitePath.localeCompare(b.sitePath),
      ),
  }));
}

export async function exportLlmsArtifacts(opts: {
  repoRoot: string;
  outDir: string;
  siteUrl: string;
  catalog: Awaited<ReturnType<typeof discoverSitePages>>;
  renderComparison?: (view: ComparisonView) => string;
}) {
  const catalog = opts.catalog;
  const pages = await Promise.all(
    catalog.pages.map(async (entry) => {
      const sourceAbsPath = resolve(opts.repoRoot, entry.sourcePath);
      const raw = await readFile(sourceAbsPath, "utf8");
      const markdown = await renderPageMarkdown({
        raw,
        sourceAbsPath,
        repoRoot: opts.repoRoot,
        siteUrl: opts.siteUrl,
        sitePages: catalog.sitePageSet,
        renderComparison: opts.renderComparison,
      });
      const dest = join(opts.outDir, entry.sitePath);
      await mkdir(dirname(dest), { recursive: true });
      await writeFile(dest, markdown, "utf8");
      return { ...entry, markdown };
    }),
  );

  const sections = pageSections(pages, catalog.explainOrder);
  const using = sections
    .filter((s) => ["home", "guide", "benchmarks"].includes(s.id))
    .flatMap((s) => s.pages);
  const explain = sections.find((s) => s.id === "explain")!.pages;
  const optional = sections
    .filter((s) => ["design", "development", "proposals"].includes(s.id))
    .flatMap((s) => s.pages);
  const firstSentence = (s: string) => {
    const m = s.match(/^(.+?[.!?])(?:\s|$)/);
    return m && m[1].length >= 25 ? m[1] : s;
  };
  const item = (p: SitePage) =>
    `- [${p.title}](${opts.siteUrl}${p.sitePath}): ${firstSentence(p.description)}`;
  const bundle = (title: string, summary: string, list: typeof pages) =>
    `${[
      `# ${title}\n\n> ${summary}`,
      ...list.map((p) => `---\n\nSource: ${opts.siteUrl}${p.sitePath}\n\n${p.markdown.trim()}`),
    ].join("\n\n")}\n`;

  const llmsTxt = `# Tessera

> ${DEFAULT_DESC}

For single-fetch AI consumption, use [llms-full.txt](${opts.siteUrl}llms-full.txt) (user guide, semantics, benchmarks, and overview) or [llms-explain.txt](${opts.siteUrl}llms-explain.txt) (algorithm deep-dive in reading order). Every page is also published as clean Markdown at its \`.md\` URL below.

## Using Tessera

${using.map(item).join("\n")}

## How Tessera Works

${explain.map(item).join("\n")}

## Optional

Internal contributor documentation, benchmark engineering notes, and unimplemented design proposals. External integration agents can skip this section.

${optional.map(item).join("\n")}
`;

  const explainOverview = explain.find((p) => p.sitePath === "explain/index.md");
  const llmsFullTxt = bundle(
    "Tessera Documentation (User Core)",
    "Single-fetch user bundle: overview, getting started, ordering semantics, benchmarks, and pipeline overview. Excludes internal contributor notes and unimplemented proposals.",
    explainOverview ? [...using, explainOverview] : using,
  );
  const llmsExplainTxt = bundle(
    "Tessera Algorithm Deep Dive",
    "Complete algorithm explanation in reading order, preserving LaTeX formulas.",
    explain,
  );

  await mkdir(opts.outDir, { recursive: true });
  await Promise.all([
    writeFile(join(opts.outDir, "llms.txt"), llmsTxt, "utf8"),
    writeFile(join(opts.outDir, "llms-full.txt"), llmsFullTxt, "utf8"),
    writeFile(join(opts.outDir, "llms-explain.txt"), llmsExplainTxt, "utf8"),
  ]);
  return { sitePaths: catalog.pages.map((p) => p.sitePath), llmsTxt, llmsFullTxt, llmsExplainTxt };
}
