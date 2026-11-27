// SPDX-License-Identifier: BSD-2-Clause

/**
 * Single authority for Tessera's site topology, repository link resolution,
 * per-page metadata, and build-time `.md` / `llms*.txt` exports. Every page's
 * title and summary derive directly from its own Markdown source (no separate
 * page-summary table), and sections group automatically by path and chapter.
 */

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join, relative, resolve, sep } from "node:path";
import type { HeadConfig } from "vitepress";
import { COMPARISON_DATA_URL, type ComparisonView } from "./comparisonMarkdown.ts";

export const REPOSITORY_URL = "https://github.com/Yikai-Liao/tessera-order";
export const HISTORY_DATA_URL = `${REPOSITORY_URL}/tree/benchmark-data/tessera`;
const DEFAULT_ORIGIN = "https://yikai-liao.github.io";
const DEFAULT_DESC = "Stable radix ordering with O(√n) auxiliary memory.";

export const SHARED_PAGES = new Set([
  "conformance/semantics.md",
  "ROADMAP.md",
  "RELEASING.md",
  "CONTRIBUTING.md",
]);
const NOT_PAGES = new Set(["docs/README.md"]);

const VUE_TAG_RE = /^<([A-Z][A-Za-z0-9]*)(?:\s+([^>]*?))?\s*\/?>(?:\s*<\/\1>)?$/;
const NOISE_LINE_RE =
  /^(?:<!--\s*SPDX-License-Identifier:[^>]*-->|<\/?div\b[^>]*>|<a\s+id="[^"]*"\s*>\s*<\/a>)$/i;

export type ComparisonRenderer = (view: ComparisonView) => string | null | undefined;
export type ResolvedRepositoryLink =
  | { kind: "page"; sitePath: string; anchor: string }
  | { kind: "repository"; repoPath: string; entryKind: "blob" | "tree"; anchor: string }
  | { kind: "passthrough" };

export const isPage = (repoPath: string): boolean =>
  repoPath.endsWith(".md") &&
  !NOT_PAGES.has(repoPath) &&
  (SHARED_PAGES.has(repoPath) ||
    (repoPath.startsWith("docs/") && !repoPath.startsWith("docs/.vitepress/")));

export const sitePath = (repoPath: string): string =>
  (repoPath.startsWith("docs/") ? repoPath.slice(5) : repoPath).replace(
    /(^|\/)README\.md$/,
    "$1index.md",
  );

export function canonicalSiteUrl(docsBase = "/tessera-order/"): string {
  const t = docsBase.trim();
  if (/^https?:\/\//i.test(t)) return t.endsWith("/") ? t : `${t}/`;
  const path = `${t.startsWith("/") ? "" : "/"}${t}`.replace(/\/+$/, "") + "/";
  return `${DEFAULT_ORIGIN}${path === "//" ? "/" : path}`;
}

export function discoverSitePages(repoRoot: string) {
  const allMarkdown: string[] = [];
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir)) {
      if (entry === "node_modules" || entry === "target" || entry.startsWith(".git")) continue;
      const full = join(dir, entry);
      if (statSync(full).isDirectory()) {
        if (entry !== ".vitepress") walk(full);
      } else if (entry.endsWith(".md")) {
        allMarkdown.push(relative(repoRoot, full).split(sep).join("/"));
      }
    }
  };
  walk(repoRoot);
  allMarkdown.sort();

  const pages: { sourcePath: string; sitePath: string }[] = [];
  const sitePageSet = new Set<string>();
  const rewrites: Record<string, string> = {};
  const srcExclude: string[] = [];
  for (const repoPath of allMarkdown) {
    if (isPage(repoPath)) {
      const served = sitePath(repoPath);
      pages.push({ sourcePath: repoPath, sitePath: served });
      sitePageSet.add(served);
      if (served !== repoPath) rewrites[repoPath] = served;
    } else srcExclude.push(repoPath);
  }
  return { allMarkdown, pages, sitePageSet, rewrites, srcExclude };
}

export function resolveRepositoryLink(
  repoRoot: string,
  sourceAbsPath: string,
  href: string,
): ResolvedRepositoryLink {
  if (!href || /^[a-z][a-z0-9+.-]*:/i.test(href) || href.startsWith("#") || href.startsWith("/")) {
    return { kind: "passthrough" };
  }
  const [rawPath, hash] = href.split("#", 2);
  const anchor = hash !== undefined ? `#${hash}` : "";
  if (!rawPath) return { kind: "passthrough" };
  let decoded: string;
  try {
    decoded = decodeURIComponent(rawPath);
  } catch {
    return { kind: "passthrough" };
  }
  const target = resolve(dirname(sourceAbsPath), decoded);
  const rel = relative(repoRoot, target).split(sep).join("/");
  if (rel.startsWith("..") || !existsSync(target)) return { kind: "passthrough" };
  if (isPage(rel)) return { kind: "page", sitePath: sitePath(rel), anchor };
  return {
    kind: "repository",
    repoPath: rel,
    entryKind: statSync(target).isDirectory() ? "tree" : "blob",
    anchor,
  };
}

export const formatRepositoryUrl = (
  r: Extract<ResolvedRepositoryLink, { kind: "repository" }>,
): string => `${REPOSITORY_URL}/${r.entryKind}/main/${r.repoPath}${r.anchor}`;

function parseFrontmatter(raw: string) {
  const m = raw.match(/^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/);
  if (!m) return { fm: {} as Record<string, string>, body: raw };
  const fm: Record<string, string> = {};
  let prefix = "";
  for (const line of m[1].split(/\r?\n/)) {
    const t = line.trim();
    if (!t || t.startsWith("#")) continue;
    const kv = t.match(/^([A-Za-z0-9_-]+):\s*(.*)$/);
    if (!kv) continue;
    const indent = line.length - line.trimStart().length;
    if (indent === 0) {
      prefix = kv[2] ? "" : `${kv[1]}.`;
      if (kv[2]) fm[kv[1]] = kv[2].trim().replace(/^(['"])(.*)\1$/, "$2");
    } else if (prefix && indent === 2 && kv[2]) {
      fm[`${prefix}${kv[1]}`] = kv[2].trim().replace(/^(['"])(.*)\1$/, "$2");
    }
  }
  return { fm, body: raw.slice(m[0].length) };
}

function scanLines(body: string, visit: (line: string, kind: "code" | "script" | "prose") => void) {
  let fence: { char: string; len: number } | null = null;
  let inScript = false;
  let prevBlank = true;
  let inIndented = false;
  for (const line of body.split(/\r?\n/)) {
    const t = line.trim();
    if (fence) {
      visit(line, "code");
      const close = t.match(/^(`{3,}|~{3,})\s*$/);
      if (close && close[1][0] === fence.char && close[1].length >= fence.len) fence = null;
      prevBlank = false;
    } else if (inScript) {
      visit(line, "script");
      if (/<\/script\s*>/i.test(t)) inScript = false;
    } else if (/^\s*(`{3,}|~{3,})/.test(line)) {
      const open = line.match(/^\s*(`{3,}|~{3,})/)!;
      fence = { char: open[1][0], len: open[1].length };
      visit(line, "code");
      prevBlank = inIndented = false;
    } else if (!t) {
      visit(line, "prose");
      prevBlank = true;
      inIndented = false;
    } else if ((prevBlank || inIndented) && (line.startsWith("\t") || line.startsWith("    "))) {
      inIndented = true;
      visit(line, "code");
      prevBlank = false;
    } else {
      inIndented = false;
      if (/^<script\b/i.test(t)) {
        visit(line, "script");
        if (!/<\/script\s*>/i.test(t)) inScript = true;
      } else {
        visit(line, "prose");
        prevBlank = false;
      }
    }
  }
}

function stripInline(text: string): string {
  const codes: string[] = [];
  return text
    .replace(/`([^`]+)`/g, (_m, s: string) => `\uE000${codes.push(s) - 1}\uE001`)
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\*\*([^*]+)\*\*|__([^_]+)__|\*([^*]+)\*/g, "$1$2$3")
    .replace(/(^|\W)_([^_]+)_(?=\W|$)/g, "$1$2")
    .replace(/<\/?[a-zA-Z][^>]*>/g, "")
    .replace(/\uE000(\d+)\uE001/g, (_m, i: string) => codes[Number(i)] ?? "")
    .replace(/\s+/g, " ")
    .trim();
}

export function extractPageMetadata(raw: string, fallback = "Tessera") {
  const { fm, body } = parseFrontmatter(raw);
  let heading = "";
  let firstPara = "";
  const buf: string[] = [];
  const flush = () => {
    if (!firstPara && buf.length) {
      const plain = stripInline(buf.join(" "));
      if (plain && !(plain.endsWith(":") && plain.length < 60)) firstPara = plain;
    }
    buf.length = 0;
  };
  scanLines(body, (line, kind) => {
    const t = line.trim();
    if (kind !== "prose" || !t || NOISE_LINE_RE.test(t) || VUE_TAG_RE.test(t)) return flush();
    if (t.startsWith("# ")) {
      flush();
      if (!heading) heading = stripInline(t.slice(2));
    } else if (/^(?:#{1,6}\s|[-*+]\s|\d+\.\s|[-*_]{3,}$|[|>]|\$\$)/.test(t)) {
      flush();
    } else if (!firstPara) buf.push(t);
  });
  flush();

  const hero =
    fm["hero.name"] && fm["hero.text"]
      ? `${fm["hero.name"]} — ${fm["hero.text"]}`
      : fm["hero.name"];
  const title = fm.title || heading || hero || fallback;
  const rawDesc = fm.description || fm["hero.tagline"] || firstPara || DEFAULT_DESC;
  const description =
    rawDesc.length <= 240
      ? rawDesc
      : (() => {
          const s = rawDesc.slice(0, 240);
          const dot = s.lastIndexOf(". ");
          if (dot >= 120) return s.slice(0, dot + 1);
          const sp = s.lastIndexOf(" ");
          return `${(sp > 0 ? s.slice(0, sp) : s).trimEnd()}…`;
        })();
  return { title, description };
}

export function buildPageHeadMeta(p: {
  relativePath: string;
  filePath: string;
  title: string;
  description: string;
  frontmatter: Record<string, unknown>;
  srcDir: string;
  siteUrl: string;
}): { description: string; head: HeadConfig[] } {
  let description = p.description?.trim() ?? "";
  let title = p.title?.trim() ?? "";
  if ((!description || !title) && p.filePath) {
    const abs = resolve(p.srcDir, p.filePath);
    if (existsSync(abs)) {
      const meta = extractPageMetadata(readFileSync(abs, "utf8"), title || "Tessera");
      description ||= meta.description;
      title ||= meta.title;
    }
  }
  description ||= DEFAULT_DESC;

  const canonical = `${p.siteUrl}${p.relativePath.replace(/(^|\/)index\.md$/, "$1").replace(/\.md$/, "")}`;
  const isHome = p.relativePath === "index.md";
  const ogTitle =
    isHome || !title || title === "Tessera" ? title || "Tessera" : `${title} | Tessera`;
  const head = Array.isArray(p.frontmatter.head) ? ([...p.frontmatter.head] as HeadConfig[]) : [];
  const add = (tag: string, attrs: Record<string, string>, matchKey: string) => {
    if (!head.some(([t, a]) => t === tag && a[matchKey] === attrs[matchKey]))
      head.push([tag, attrs]);
  };
  add("link", { rel: "canonical", href: canonical }, "rel");
  add(
    "link",
    { rel: "alternate", type: "text/markdown", href: `${p.siteUrl}${p.relativePath}` },
    "type",
  );
  add("meta", { property: "og:title", content: ogTitle }, "property");
  add("meta", { property: "og:description", content: description }, "property");
  add("meta", { property: "og:url", content: canonical }, "property");
  add("meta", { property: "og:type", content: isHome ? "website" : "article" }, "property");
  return { description, head };
}

export function renderPageMarkdown(p: {
  raw: string;
  sourceAbsPath: string;
  repoRoot: string;
  siteUrl: string;
  sitePages: ReadonlySet<string>;
  renderComparison?: ComparisonRenderer;
}): string {
  const { fm, body } = parseFrontmatter(p.raw);
  const out: string[] = [];
  if (fm["hero.name"] || fm["hero.text"]) {
    out.push(
      `# ${fm["hero.name"] && fm["hero.text"] ? `${fm["hero.name"]} — ${fm["hero.text"]}` : fm["hero.name"] || fm["hero.text"]}`,
    );
    if (fm["hero.tagline"]) out.push("", fm["hero.tagline"]);
    out.push("");
  }

  const rewriteHref = (href: string) => {
    if (href.startsWith("/") && !href.startsWith("//")) {
      const [rawPath, hash] = href.slice(1).split("#", 2);
      const candidates = !rawPath
        ? ["index.md"]
        : rawPath.endsWith("/")
          ? [`${rawPath}index.md`]
          : rawPath.endsWith(".md")
            ? [rawPath]
            : [`${rawPath}.md`, `${rawPath}/index.md`];
      const hit = candidates.find((c) => p.sitePages.has(c));
      if (hit) return `${p.siteUrl}${hit}${hash !== undefined ? `#${hash}` : ""}`;
    }
    const r = resolveRepositoryLink(p.repoRoot, p.sourceAbsPath, href);
    if (r.kind === "page") return `${p.siteUrl}${r.sitePath}${r.anchor}`;
    if (r.kind === "repository") return formatRepositoryUrl(r);
    return href;
  };
  const wrap = (d: string) =>
    d.startsWith("<") && d.endsWith(">") ? `<${rewriteHref(d.slice(1, -1))}>` : rewriteHref(d);

  scanLines(body, (line, kind) => {
    if (kind === "script") return;
    if (kind === "code") return void out.push(line);
    const t = line.trim();
    if (NOISE_LINE_RE.test(t)) return;
    const vue = t.match(VUE_TAG_RE);
    if (vue) {
      if (vue[1] === "ImplementationComparison") {
        const view =
          (vue[2]?.match(/\bview\s*=\s*["'](headlines|overview|all)["']/)?.[1] as
            | ComparisonView
            | undefined) ?? "all";
        const rendered = p.renderComparison?.(view)?.trim();
        if (rendered) out.push(rendered);
        else if (view !== "headlines")
          out.push(`Live benchmark comparison data: [comparison.json](${COMPARISON_DATA_URL}).`);
      } else if (vue[1] === "PerformanceHistory") {
        out.push(
          `Interactive main-branch benchmark history data: [benchmark-data/tessera](${HISTORY_DATA_URL}).`,
        );
      }
      return;
    }
    const refDef = line.match(/^(\s*\[[^\]]+\]:\s*)(<[^>]+>|\S+)(\s*.*)$/);
    out.push(
      refDef
        ? `${refDef[1]}${wrap(refDef[2])}${refDef[3]}`
        : line.replace(
            /(`+)[^\n]*?\1|(\[(?:[^[\]`]|`[^`]*`|\[[^\]]*\])*\])\((<[^>]+>|[^()\s]+(?:\([^()\s]*\)[^()\s]*)*)([^)]*)\)/g,
            (full, ticks, label, dest, tail) => (ticks ? full : `${label}(${wrap(dest)}${tail})`),
          ),
    );
  });
  return `${out
    .join("\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim()}\n`;
}

type Page = { sitePath: string; title: string; description: string; markdown: string };

function groupPages(pages: readonly Page[]) {
  const rank = (s: string) =>
    s === "index.md"
      ? 0
      : s.startsWith("guide/")
        ? 1
        : s.startsWith("conformance/")
          ? 2
          : s === "benchmarks/implementations.md"
            ? 3
            : s.startsWith("benchmarks/")
              ? 4
              : -1;
  const chapter = (p: Page) =>
    p.sitePath === "explain/index.md" ? 0 : Number(p.title.match(/^(\d+)\./)?.[1] ?? 999);
  return {
    using: pages
      .filter((p) => rank(p.sitePath) >= 0)
      .sort((a, b) => rank(a.sitePath) - rank(b.sitePath) || a.sitePath.localeCompare(b.sitePath)),
    explain: pages
      .filter((p) => p.sitePath.startsWith("explain/"))
      .sort((a, b) => chapter(a) - chapter(b) || a.sitePath.localeCompare(b.sitePath)),
    optional: pages
      .filter((p) => rank(p.sitePath) < 0 && !p.sitePath.startsWith("explain/"))
      .sort((a, b) => a.sitePath.localeCompare(b.sitePath)),
  };
}

export async function exportLlmsArtifacts(opts: {
  repoRoot: string;
  outDir: string;
  siteUrl: string;
  catalog?: ReturnType<typeof discoverSitePages>;
  renderComparison?: ComparisonRenderer;
}) {
  const catalog = opts.catalog ?? discoverSitePages(opts.repoRoot);
  const pages: Page[] = await Promise.all(
    catalog.pages.map(async (entry) => {
      const sourceAbsPath = resolve(opts.repoRoot, entry.sourcePath);
      const raw = await readFile(sourceAbsPath, "utf8");
      const meta = extractPageMetadata(raw, entry.sitePath);
      const markdown = renderPageMarkdown({
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
      return { sitePath: entry.sitePath, ...meta, markdown };
    }),
  );

  const { using, explain, optional } = groupPages(pages);
  const firstSentence = (s: string) => {
    const m = s.match(/^(.+?[.!?])(?:\s|$)/);
    return m && m[1].length >= 25 ? m[1] : s;
  };
  const item = (p: Page) =>
    `- [${p.title}](${opts.siteUrl}${p.sitePath}): ${firstSentence(p.description)}`;
  const bundle = (title: string, summary: string, list: readonly Page[]) =>
    [
      `# ${title}\n\n> ${summary}`,
      ...list.map((p) => `---\n\nSource: ${opts.siteUrl}${p.sitePath}\n\n${p.markdown.trim()}`),
    ].join("\n\n") + "\n";

  const llmsTxt = `${[
    "# Tessera",
    "",
    `> ${DEFAULT_DESC}`,
    "",
    `For single-fetch AI consumption, use [llms-full.txt](${opts.siteUrl}llms-full.txt) (user guide, semantics, benchmarks, and overview) or [llms-explain.txt](${opts.siteUrl}llms-explain.txt) (algorithm deep-dive in reading order). Every page is also published as clean Markdown at its \`.md\` URL below.`,
    "",
    "## Using Tessera",
    "",
    ...using.map(item),
    "",
    "## How Tessera Works",
    "",
    ...explain.map(item),
    "",
    "## Optional",
    "",
    "Internal contributor documentation, benchmark engineering notes, and unimplemented design proposals. External integration agents can skip this section.",
    "",
    ...optional.map(item),
  ].join("\n")}\n`;

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
