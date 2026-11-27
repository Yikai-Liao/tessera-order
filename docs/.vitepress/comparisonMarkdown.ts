// SPDX-License-Identifier: BSD-2-Clause

/**
 * Single authority for Tessera's published `comparison.json` data across the site:
 * source URL, build-time loading and validation, headline/provenance/section
 * derivation, Markdown table parsing, and standalone Markdown rendering.
 *
 * Browser-safe by construction: this module has no static `node:*` runtime imports,
 * so VitePress components can import its pure helpers directly while Node/Bun build
 * steps (`comparisonSnapshot.data.ts` and `llms.ts`) call `loadComparisonSnapshot()`.
 *
 * Failure semantics:
 * - `parseComparisonData()` throws a Zod error on malformed input.
 * - `loadComparisonSnapshot()` returns `null` when offline, timed out, unreachable,
 *   or invalid so documentation builds and tests never fail on network state.
 */

import type * as fsPromises from "node:fs/promises";
import type * as nodePath from "node:path";
import type * as nodeUrl from "node:url";
import { formatBytes, ordinal, times } from "../../bench/report/format.ts";
import {
  type ComparisonData,
  type ComparisonHeadline,
  comparisonSchema,
} from "../../bench/tracking/result.ts";

/** Published cross-implementation benchmark snapshot on the `benchmark-data` branch. */
export const COMPARISON_DATA_URL =
  "https://raw.githubusercontent.com/Yikai-Liao/tessera-order/benchmark-data/comparison.json";

const COMMIT_URL_PREFIX = "https://github.com/Yikai-Liao/tessera-order/commit/";
const DEFAULT_TIMEOUT_MS = 5000;

export type ComparisonView = "overview" | "all";
export type ComparisonFigure = ComparisonData["operations"][number]["figures"][number];
export type ComparisonRun = ComparisonData["operations"][number]["runs"][number];

export interface ComparisonHeadlineCard {
  id: string;
  headline: ComparisonHeadline;
  claim: string;
  speedText: string;
  memoryText: string | null;
  referenceMemoryText: string | null;
}

export interface ComparisonProvenance {
  text: string;
  tessera: { short: string; link: string } | null;
  unmeasured: string[];
}

export interface ComparisonSection {
  id: string;
  label: string;
  provenance: ComparisonProvenance | null;
  figures: ComparisonFigure[];
}

export interface MarkdownTable {
  head: string[];
  body: string[][];
}

export interface LoadComparisonOptions {
  /** Override environment variables (defaults to `process.env`). */
  env?: Record<string, string | undefined>;
  /** Override the `docs/public` directory for relative `VITE_COMPARISON_SOURCE` overrides. */
  publicDir?: string;
  /** Timeout in milliseconds for remote fetches (defaults to 5000). */
  timeoutMs?: number;
  /** Override `fetch` for tests. */
  fetch?: typeof globalThis.fetch;
}

/** Validate raw JSON against `comparisonSchema`. */
export function parseComparisonData(input: unknown): ComparisonData {
  return comparisonSchema.parse(input);
}

/** Derive formatted headline summary cards for every operation that has one. */
export function comparisonHeadlines(data: ComparisonData): ComparisonHeadlineCard[] {
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

/** Derive date span, Tessera commit link, and unmeasured-memory labels from a section's runs. */
export function comparisonProvenance(runs: readonly ComparisonRun[]): ComparisonProvenance | null {
  if (!runs.length) return null;
  const days = runs.map((run) => run.time.slice(0, 10)).toSorted();
  const span = days[0] === days.at(-1) ? days[0] : `${days[0]} to ${days.at(-1)}`;
  const tessera = runs.find((run) => run.implementation === "tessera");
  return {
    text: `Latest run of each implementation on Bencher Intel v1 bare metal, ${span}${tessera ? "; Tessera at" : ""}`,
    tessera: tessera
      ? {
          short: tessera.hash.slice(0, 7),
          link: `${COMMIT_URL_PREFIX}${tessera.hash}`,
        }
      : null,
    unmeasured: runs.filter((run) => !run.memory).map((run) => run.label),
  };
}

/** Select the sections and figures shown for `overview` (sort ranking) or `all` (every operation). */
export function comparisonSections(
  data: ComparisonData,
  view: ComparisonView,
): ComparisonSection[] {
  if (view === "overview") {
    const sort = data.operations.find((operation) => operation.id === "sort");
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
  return data.operations.map((operation) => ({
    id: operation.id,
    label: operation.label,
    provenance: comparisonProvenance(operation.runs),
    figures: operation.figures,
  }));
}

/** Parse a GFM Markdown table into plain-text header and body cells. */
export function parseMarkdownTable(markdown: string): MarkdownTable {
  const trimmed = markdown.trim();
  if (!trimmed) return { head: [], body: [] };
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

function escapeTableCell(text: string): string {
  return text.replace(/\s+/g, " ").trim().replaceAll("|", "\\|");
}

function formatProvenanceMarkdown(provenance: ComparisonProvenance): string {
  const commit = provenance.tessera
    ? ` [${provenance.tessera.short}](${provenance.tessera.link})`
    : "";
  const unmeasured = provenance.unmeasured.length
    ? ` Memory not yet recorded for ${provenance.unmeasured.join(", ")}.`
    : "";
  return `${provenance.text}${commit}.${unmeasured}`;
}

function formatFigureLead(figure: ComparisonFigure, includeTitle: boolean): string {
  const title = includeTitle ? figure.title.trim() : "";
  const summary = figure.summary?.trim() ?? "";
  if (title && summary) {
    const punctuated = /[.!?]$/.test(title) ? title : `${title}.`;
    return `**${punctuated}** ${summary}`;
  }
  if (title) return `**${title}**`;
  return summary;
}

/** Render a validated comparison snapshot into standalone Markdown for `overview` or `all`. */
export function renderComparisonMarkdown(data: ComparisonData, view: ComparisonView): string {
  const sections = comparisonSections(data, view);
  if (!sections.length) return "No cross-implementation figure is published yet.";

  const blocks: string[] = [];

  if (view === "overview") {
    const [section] = sections;
    const [figure] = section.figures;
    const lead = formatFigureLead(figure, true);
    if (lead) blocks.push(lead);
    const table = figure.table.trim();
    if (table) blocks.push(table);
    if (section.provenance) blocks.push(formatProvenanceMarkdown(section.provenance));
    return blocks.join("\n\n");
  }

  const cards = comparisonHeadlines(data);
  if (cards.length) {
    const rows = cards.map((card) => {
      const memory =
        card.memoryText === null
          ? "—"
          : card.referenceMemoryText === null
            ? card.memoryText
            : `${card.memoryText} (vs ${card.referenceMemoryText})`;
      return `| ${escapeTableCell(card.headline.operation)} | ${escapeTableCell(card.claim)} | ${escapeTableCell(card.speedText)} | ${escapeTableCell(memory)} |`;
    });
    blocks.push(
      "## At a glance",
      [
        "| Operation | Standing | Speed vs std | Extra memory |",
        "| --- | --- | --- | --- |",
        ...rows,
      ].join("\n"),
    );
  }

  for (const section of sections) {
    blocks.push(`## ${section.label}`);
    if (section.provenance) blocks.push(formatProvenanceMarkdown(section.provenance));
    for (const figure of section.figures) {
      const heading = figure.subject ?? figure.title;
      blocks.push(`### ${heading}`);
      const includeTitle = Boolean(figure.subject && figure.subject !== figure.title);
      const lead = formatFigureLead(figure, includeTitle);
      if (lead) blocks.push(lead);
      const table = figure.table.trim();
      if (table) blocks.push(table);
    }
  }

  return blocks.join("\n\n");
}

interface NodeBuiltins {
  fs: typeof fsPromises;
  path: typeof nodePath;
  url: typeof nodeUrl;
}

function getNodeBuiltins(): NodeBuiltins | null {
  const getBuiltin = (
    globalThis.process as { getBuiltinModule?: (id: string) => unknown } | undefined
  )?.getBuiltinModule;
  if (typeof getBuiltin !== "function") return null;
  const fs = getBuiltin("node:fs/promises") as typeof fsPromises | undefined;
  const path = getBuiltin("node:path") as typeof nodePath | undefined;
  const url = getBuiltin("node:url") as typeof nodeUrl | undefined;
  if (!fs || !path || !url) return null;
  return { fs, path, url };
}

async function readComparisonFile(
  builtins: NodeBuiltins,
  filePath: string,
): Promise<ComparisonData | null> {
  try {
    const raw = await builtins.fs.readFile(filePath, "utf8");
    return parseComparisonData(JSON.parse(raw));
  } catch {
    return null;
  }
}

async function fetchComparisonUrl(
  url: string,
  timeoutMs: number,
  fetchImpl: typeof globalThis.fetch,
): Promise<ComparisonData | null> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const response = await fetchImpl(url, { signal: controller.signal });
    if (!response.ok) return null;
    return parseComparisonData(await response.json());
  } catch {
    return null;
  } finally {
    clearTimeout(timer);
  }
}

let cachedDefaultRemote: Promise<ComparisonData | null> | null = null;

/**
 * Load and validate the build-time `comparison.json` snapshot.
 *
 * Resolution order:
 * 1. `DOCS_COMPARISON_FILE` — local filesystem path (never falls back to network on error).
 * 2. `VITE_COMPARISON_SOURCE` — `http(s)://` URL or path relative to `docs/public/`
 *    (never falls back to the default remote URL on error).
 * 3. `COMPARISON_DATA_URL` — remote fetch with timeout (memoized per process for default
 *    no-argument calls so SSR pre-hydration and `buildEnd` share one snapshot).
 *
 * Returns `null` when offline, timed out, unreachable, or invalid.
 */
export async function loadComparisonSnapshot(
  options?: LoadComparisonOptions,
): Promise<ComparisonData | null> {
  const env = options?.env ?? globalThis.process?.env ?? {};
  const timeoutMs = options?.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const fetchImpl = options?.fetch ?? globalThis.fetch;

  const fileOverride = env.DOCS_COMPARISON_FILE?.trim();
  if (fileOverride) {
    const builtins = getNodeBuiltins();
    if (!builtins) return null;
    return readComparisonFile(builtins, builtins.path.resolve(fileOverride));
  }

  const sourceOverride = env.VITE_COMPARISON_SOURCE?.trim();
  if (sourceOverride) {
    if (/^https?:\/\//i.test(sourceOverride)) {
      return fetchComparisonUrl(sourceOverride, timeoutMs, fetchImpl);
    }
    const builtins = getNodeBuiltins();
    if (!builtins) return null;
    const publicDir = builtins.path.resolve(
      options?.publicDir ??
        builtins.path.resolve(
          builtins.path.dirname(builtins.url.fileURLToPath(import.meta.url)),
          "../public",
        ),
    );
    const relativeSource = sourceOverride.replace(/^\/+/, "");
    const resolved = builtins.path.resolve(publicDir, relativeSource);
    const rel = builtins.path.relative(publicDir, resolved);
    if (!rel || rel.startsWith("..") || builtins.path.isAbsolute(rel)) return null;
    return readComparisonFile(builtins, resolved);
  }

  if (!options) {
    cachedDefaultRemote ??= fetchComparisonUrl(COMPARISON_DATA_URL, timeoutMs, fetchImpl);
    return cachedDefaultRemote;
  }

  return fetchComparisonUrl(COMPARISON_DATA_URL, timeoutMs, fetchImpl);
}
