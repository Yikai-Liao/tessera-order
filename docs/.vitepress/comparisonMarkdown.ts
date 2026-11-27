// SPDX-License-Identifier: BSD-2-Clause

/** Shared presentation of the published comparison, using the benchmark schema and formatters. */

import { formatBytes, ordinal, times } from "../../bench/report/format.ts";
import type { ComparisonData } from "../../bench/tracking/result.ts";

export const COMPARISON_DATA_URL =
  "https://raw.githubusercontent.com/Yikai-Liao/tessera-order/benchmark-data/comparison.json";

const COMMIT_PREFIX = "https://github.com/Yikai-Liao/tessera-order/commit/";
export type ComparisonView = "headlines" | "overview" | "all";
export type ComparisonFigure = ComparisonData["operations"][number]["figures"][number];
export type ComparisonRun = ComparisonData["operations"][number]["runs"][number];

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

/** Decode the fixed pipe-table format emitted by bench/report/charts.ts for live figures. */
export function comparisonTable(markdown: string) {
  if (!markdown.trim()) return { head: [] as string[], body: [] as string[][] };
  const [head, , ...body] = markdown
    .trim()
    .split("\n")
    .map((line) =>
      line
        .trim()
        .slice(1, -1)
        .split(/(?<!\\)\|/)
        .map((cell) => cell.trim().replaceAll("\\|", "|")),
    );
  return { head, body };
}

/** Render plain values as a Markdown table, escaping cell boundaries. */
export function markdownTable(headers: string[], rows: string[][]): string {
  const cell = (s: string) => s.replace(/\s+/g, " ").trim().replaceAll("|", "\\|");
  return [headers, headers.map(() => "---"), ...rows]
    .map((row) => `| ${row.map(cell).join(" | ")} |`)
    .join("\n");
}

function renderHeadlinesTable(cards: ReturnType<typeof comparisonHeadlines>): string {
  return markdownTable(
    ["Operation", "Standing", "Speed vs std", "Extra memory"],
    cards.map((c) => [
      c.headline.operation,
      c.claim,
      c.speedText,
      c.memoryText === null
        ? "—"
        : c.referenceMemoryText === null
          ? c.memoryText
          : `${c.memoryText} (vs ${c.referenceMemoryText})`,
    ]),
  );
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
