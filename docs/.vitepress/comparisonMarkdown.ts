// SPDX-License-Identifier: BSD-2-Clause

/**
 * Single authority for Tessera's published `comparison.json` data across the site:
 * source URL, build-time loading and validation, headline/provenance/section
 * derivation, Markdown table parsing, and standalone Markdown rendering.
 *
 * Browser-safe: no static `node:*` runtime imports, so Vue components import pure
 * helpers directly while build steps call `loadComparisonSnapshot()`.
 */

import type * as fsPromises from "node:fs/promises";
import type * as nodePath from "node:path";
import type * as nodeUrl from "node:url";
import { formatBytes, ordinal, times } from "../../bench/report/format.ts";
import { type ComparisonData, comparisonSchema } from "../../bench/tracking/result.ts";

export const COMPARISON_DATA_URL =
  "https://raw.githubusercontent.com/Yikai-Liao/tessera-order/benchmark-data/comparison.json";

const COMMIT_PREFIX = "https://github.com/Yikai-Liao/tessera-order/commit/";
const DEFAULT_TIMEOUT_MS = 5000;

export type ComparisonView = "headlines" | "overview" | "all";
export type ComparisonFigure = ComparisonData["operations"][number]["figures"][number];
export type ComparisonRun = ComparisonData["operations"][number]["runs"][number];

export function parseComparisonData(input: unknown): ComparisonData {
  return comparisonSchema.parse(input);
}

export function comparisonHeadlines(data: ComparisonData) {
  return data.operations.flatMap(({ id, headline }) => {
    if (!headline) return [];
    const standing = headline.rank === 1 ? "Fastest" : `Ranked ${ordinal(headline.rank)}`;
    return [
      {
        id,
        headline,
        claim: `${standing} of ${headline.of} stable ${headline.noun}`,
        speedText: times(headline.speed),
        memoryText: headline.memoryBytes === null ? null : formatBytes(headline.memoryBytes),
        referenceMemoryText:
          headline.referenceMemoryBytes === null
            ? null
            : formatBytes(headline.referenceMemoryBytes),
      },
    ];
  });
}

export function comparisonProvenance(runs: readonly ComparisonRun[]) {
  if (!runs.length) return null;
  const days = runs.map((run) => run.time.slice(0, 10)).toSorted();
  const span = days[0] === days.at(-1) ? days[0] : `${days[0]} to ${days.at(-1)}`;
  const tessera = runs.find((run) => run.implementation === "tessera");
  return {
    text: `Latest run of each implementation on Bencher Intel v1 bare metal, ${span}${tessera ? "; Tessera at" : ""}`,
    tessera: tessera
      ? { short: tessera.hash.slice(0, 7), link: `${COMMIT_PREFIX}${tessera.hash}` }
      : null,
    unmeasured: runs.filter((run) => !run.memory).map((run) => run.label),
  };
}

export function comparisonSections(data: ComparisonData, view: ComparisonView) {
  if (view === "headlines") return [];
  if (view === "overview") {
    const sort = data.operations.find((op) => op.id === "sort");
    return data.overview && sort
      ? [
          {
            id: sort.id,
            label: sort.label,
            provenance: comparisonProvenance(sort.runs),
            figures: [data.overview],
          },
        ]
      : [];
  }
  return data.operations.map((op) => ({
    id: op.id,
    label: op.label,
    provenance: comparisonProvenance(op.runs),
    figures: op.figures,
  }));
}

export function parseMarkdownTable(markdown: string) {
  const trimmed = markdown.trim();
  if (!trimmed) return { head: [] as string[], body: [] as string[][] };
  const rows = trimmed
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line !== "" && !/^\|\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)*\|?$/.test(line))
    .map((line) =>
      line
        .replace(/^\|/, "")
        .replace(/\|$/, "")
        .split(/(?<!\\)\|/)
        .map((cell) => cell.trim().replaceAll("\\|", "|")),
    );
  return { head: rows[0] ?? [], body: rows.slice(1) };
}

const escCell = (s: string) => s.replace(/\s+/g, " ").trim().replaceAll("|", "\\|");

function renderHeadlinesTable(cards: ReturnType<typeof comparisonHeadlines>): string {
  const rows = cards.map((c) => {
    const mem =
      c.memoryText === null
        ? "—"
        : c.referenceMemoryText === null
          ? c.memoryText
          : `${c.memoryText} (vs ${c.referenceMemoryText})`;
    return `| ${escCell(c.headline.operation)} | ${escCell(c.claim)} | ${escCell(c.speedText)} | ${escCell(mem)} |`;
  });
  return [
    "| Operation | Standing | Speed vs std | Extra memory |",
    "| --- | --- | --- | --- |",
    ...rows,
  ].join("\n");
}

function formatProvenance(p: NonNullable<ReturnType<typeof comparisonProvenance>>): string {
  const commit = p.tessera ? ` [${p.tessera.short}](${p.tessera.link})` : "";
  const extra = p.unmeasured.length
    ? ` Memory not yet recorded for ${p.unmeasured.join(", ")}.`
    : "";
  return `${p.text}${commit}.${extra}`;
}

function formatLead(figure: ComparisonFigure, includeTitle: boolean): string {
  const title = includeTitle ? figure.title.trim() : "";
  const summary = figure.summary?.trim() ?? "";
  if (title && summary) return `**${/[.!?]$/.test(title) ? title : `${title}.`}** ${summary}`;
  return title ? `**${title}**` : summary;
}

export function renderComparisonMarkdown(data: ComparisonData, view: ComparisonView): string {
  const cards = comparisonHeadlines(data);
  if (view === "headlines") return cards.length ? renderHeadlinesTable(cards) : "";

  const sections = comparisonSections(data, view);
  if (!sections.length) return "No cross-implementation figure is published yet.";

  const blocks: string[] = [];
  if (view === "overview") {
    const [section] = sections;
    const [figure] = section.figures;
    const lead = formatLead(figure, true);
    if (lead) blocks.push(lead);
    if (figure.table.trim()) blocks.push(figure.table.trim());
    if (section.provenance) blocks.push(formatProvenance(section.provenance));
    return blocks.join("\n\n");
  }

  if (cards.length) blocks.push("## At a glance", renderHeadlinesTable(cards));
  for (const section of sections) {
    blocks.push(`## ${section.label}`);
    if (section.provenance) blocks.push(formatProvenance(section.provenance));
    for (const figure of section.figures) {
      blocks.push(`### ${figure.subject ?? figure.title}`);
      const lead = formatLead(figure, Boolean(figure.subject && figure.subject !== figure.title));
      if (lead) blocks.push(lead);
      if (figure.table.trim()) blocks.push(figure.table.trim());
    }
  }
  return blocks.join("\n\n");
}

function nodeBuiltins() {
  const get = (globalThis.process as { getBuiltinModule?: (id: string) => unknown } | undefined)
    ?.getBuiltinModule;
  if (typeof get !== "function") return null;
  const fs = get("node:fs/promises") as typeof fsPromises | undefined;
  const path = get("node:path") as typeof nodePath | undefined;
  const url = get("node:url") as typeof nodeUrl | undefined;
  return fs && path && url ? { fs, path, url } : null;
}

async function fetchJson(
  url: string,
  timeoutMs: number,
  fetchImpl: typeof globalThis.fetch,
): Promise<ComparisonData | null> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const res = await fetchImpl(url, { signal: controller.signal });
    return res.ok ? parseComparisonData(await res.json()) : null;
  } catch {
    return null;
  } finally {
    clearTimeout(timer);
  }
}

let cachedDefaultRemote: Promise<ComparisonData | null> | null = null;

/** Load `comparison.json` from `DOCS_COMPARISON_FILE`, `VITE_COMPARISON_SOURCE`, or `COMPARISON_DATA_URL`. */
export async function loadComparisonSnapshot(options?: {
  env?: Record<string, string | undefined>;
  publicDir?: string;
  timeoutMs?: number;
  fetch?: typeof globalThis.fetch;
}): Promise<ComparisonData | null> {
  const env = options?.env ?? globalThis.process?.env ?? {};
  const timeoutMs = options?.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const fetchImpl = options?.fetch ?? globalThis.fetch;

  const readLocal = async (filePath: string) => {
    const node = nodeBuiltins();
    if (!node) return null;
    try {
      return parseComparisonData(JSON.parse(await node.fs.readFile(filePath, "utf8")));
    } catch {
      return null;
    }
  };

  const fileOverride = env.DOCS_COMPARISON_FILE?.trim();
  if (fileOverride) {
    const node = nodeBuiltins();
    return node ? readLocal(node.path.resolve(fileOverride)) : null;
  }

  const sourceOverride = env.VITE_COMPARISON_SOURCE?.trim();
  if (sourceOverride) {
    if (/^https?:\/\//i.test(sourceOverride))
      return fetchJson(sourceOverride, timeoutMs, fetchImpl);
    const node = nodeBuiltins();
    if (!node) return null;
    const publicDir = node.path.resolve(
      options?.publicDir ??
        node.path.resolve(node.path.dirname(node.url.fileURLToPath(import.meta.url)), "../public"),
    );
    const resolved = node.path.resolve(publicDir, sourceOverride.replace(/^\/+/, ""));
    const rel = node.path.relative(publicDir, resolved);
    if (!rel || rel.startsWith("..") || node.path.isAbsolute(rel)) return null;
    return readLocal(resolved);
  }

  if (!options) {
    cachedDefaultRemote ??= fetchJson(COMPARISON_DATA_URL, timeoutMs, fetchImpl);
    return cachedDefaultRemote;
  }
  return fetchJson(COMPARISON_DATA_URL, timeoutMs, fetchImpl);
}
