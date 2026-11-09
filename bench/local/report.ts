// SPDX-License-Identifier: BSD-2-Clause

import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { renderRows } from "../plot/rows.ts";
import { type LayoutRun, summarizeLayouts } from "./layout.ts";
import { type LocalRun, summarizeLocal } from "./model.ts";
import type { ProtocolRun } from "./protocol.ts";
import { renderProtocolReport } from "./protocol-report.ts";

/** Show absolute times and repeatability for each case, without a revision-ratio axis. */
async function writeLayoutFigures(
  directory: string,
  summaries: ReturnType<typeof summarizeLayouts>,
) {
  const figures: string[] = [];
  for (const [index, summary] of summaries.entries()) {
    const unit = summary.layouts[0].baselineNs >= 1000 ? 1000 : 1;
    const label = unit === 1000 ? "µs" : "ns";
    const rows = summary.layouts.flatMap((layout) => [
      {
        seed: layout.seed,
        role: "baseline",
        value: layout.baselineNs / unit,
        low: layout.baselineLowNs / unit,
        high: layout.baselineHighNs / unit,
      },
      {
        seed: layout.seed,
        role: "current",
        value: layout.currentNs / unit,
        low: layout.currentLowNs / unit,
        high: layout.currentHighNs / unit,
      },
    ]);
    const low = Math.min(...rows.map((row) => row.low)) * 0.97;
    const high = Math.max(...rows.map((row) => row.high)) * 1.03;
    const filename = `layout-${index + 1}.svg`;
    const svg = renderRows({
      title: `Layout times · ${summary.id.split("/").slice(1, 4).join(" / ")}`,
      guide: "Median time per sort · lines: observed nine-block ranges, not confidence intervals",
      columns: [`Median (${label})`, `Block range (${label})`],
      groups: [
        {
          title: "Fixed shuffle seeds",
          rows: rows.map((row) => ({
            name: { input: `Seed ${row.seed} · ${row.role}`, sub: "", tag: "" },
            marks: [{ value: row.value, tone: "primary" }],
            span: [row.low, row.high],
            cells: [row.value.toFixed(2), `${row.low.toFixed(2)}–${row.high.toFixed(2)}`],
          })),
        },
      ],
      scale: {
        low,
        high,
        ticks: Array.from({ length: 5 }, (_, index) => {
          const value = low * (high / low) ** (index / 4);
          return { value, label: value.toFixed(1) };
        }),
        clamp: false,
      },
      legend: {
        note: "baseline: median of paired baseline means · current: median of current processes",
      },
      axis: ["← less time", `${label} per sort →`],
    });
    await writeFile(join(directory, filename), svg);
    figures.push(`![Absolute layout times for ${summary.id}](${filename})`);
  }
  return figures.join("\n\n");
}

/** Write a local comparison's table, provenance and process-level control chart. */
export async function renderLocalReport(
  directory: string,
  input?: LocalRun | LayoutRun | ProtocolRun,
) {
  const result =
    input ??
    (JSON.parse(await readFile(join(directory, "local.json"), "utf8")) as
      | LocalRun
      | LayoutRun
      | ProtocolRun);
  if (result.schema === 3) return renderProtocolReport(directory, result);
  if (result.schema !== 1 && result.schema !== 2)
    throw new Error("Unsupported local result schema");
  const layoutSummaries = result.schema === 2 ? summarizeLayouts(result) : undefined;
  const layoutFigures = layoutSummaries ? await writeLayoutFigures(directory, layoutSummaries) : "";
  const runs =
    result.schema === 2
      ? result.layouts.map(({ seed, run }) => ({
          title: `Layout seed ${seed} · ${run.configuration}`,
          run,
        }))
      : [{ title: result.configuration, run: result }];
  const run = runs[0].run;
  const groups = runs.map(({ title, run }) => ({ title, summaries: summarizeLocal(run) }));
  const summaries = groups.flatMap(({ summaries }) => summaries);
  const ratios = summaries.flatMap((s) => [s.low, s.high, s.aaLow, s.aaHigh]);
  const reach = 2 ** Math.ceil(Math.log2(Math.max(2, ...ratios.map((v) => Math.max(v, 1 / v)))));
  const ticks = [];
  for (let k = -Math.log2(reach); k <= Math.log2(reach); k++)
    ticks.push({ value: 2 ** k, label: `${2 ** k}×` });
  const svg = renderRows({
    title: "Working tree against commit",
    guide: "Current / baseline run time · lower is better · nine process-level blocks per layout",
    columns: ["Time ratio", "A/A range"],
    groups: groups.map(({ title, summaries }) => ({
      title,
      rows: summaries.map((s) => ({
        name: {
          input: s.id.split("/").slice(1, 3).join(" · "),
          sub: s.id.split("/").slice(3).join(" · "),
          tag: "",
        },
        marks: [{ value: s.ratio, tone: "primary" }],
        span: [s.low, s.high],
        cells: [`${s.ratio.toFixed(4)}×`, `${s.aaLow.toFixed(4)}–${s.aaHigh.toFixed(4)}`],
        strong: true,
      })),
    })),
    scale: { low: 1 / reach, high: reach, ticks, origin: 1, clamp: false },
    legend: { note: "dot: median · line: block range" },
    axis: ["← less time", "more time →"],
  });
  const table = [
    "| Layout | Case | Current / baseline | Block range | A/A range | Screen |",
    "| --- | --- | --- | --- | --- | --- |",
    ...groups.flatMap(({ title, summaries }) =>
      summaries.map(
        (s) =>
          `| ${title} | ${s.id} | ${s.ratio.toFixed(4)} | ${s.low.toFixed(4)}–${s.high.toFixed(4)} | ${s.aaLow.toFixed(4)}–${s.aaHigh.toFixed(4)} | ${s.verdict} |`,
      ),
    ),
  ].join("\n");
  const layoutTable = layoutSummaries
    ? [
        "| Case | Median ratio across layouts | Observed ratio range | Baseline spread | Current spread |",
        "| --- | --- | --- | --- | --- |",
        ...layoutSummaries.map(
          (s) =>
            `| ${s.id} | ${s.ratio.toFixed(4)} | ${s.low.toFixed(4)}–${s.high.toFixed(4)} | ${(s.baselineSpread * 100).toFixed(2)}% | ${(s.currentSpread * 100).toFixed(2)}% |`,
        ),
      ].join("\n")
    : "";
  const markdown = `# Local comparison

Baseline: \`${run.baseline}\`. Working-tree HEAD: \`${run.head}\` (${run.dirty ? "modified" : "clean"}).

This is a development screen, not an in-process source-attribution claim. Each process measures Criterion's mean time per sort. Each block runs the baseline executable twice and the current executable once; ratios use the paired baseline mean. A direction requires eight of nine blocks and a median outside the A/A range. All processes use the same input seed and current harness; they do not share an address space.

${
  layoutSummaries
    ? `Build control: ${result.schema === 2 ? (result.control ?? "none") : "none"}. Each fixed seed shuffles LLD's .text.* input sections for both revisions. Seeds rotate between blocks; every seed retains nine complete blocks and its own process-level A/A control. Equal seeds do not imply matching addresses between revisions. The cross-layout median and min/max describe only the observed sample, with no pooled regression verdict or confidence interval. Spreads are max/min minus one across each revision's per-layout median times; they include process and host variation. Link maps are retained as layout-<seed>-<revision>.map. Final rustc arguments are recorded separately from Cargo's dependency flags.

${layoutTable}

${layoutFigures}
`
    : ""
}

![Working tree against commit](local.svg)

${table}

## Provenance

\`\`\`json
${JSON.stringify(result.schema === 2 ? { ...result, layouts: result.layouts.map(({ seed, run }) => ({ seed, run: { ...run, blocks: undefined } })) } : { ...run, blocks: undefined }, null, 2)}
\`\`\`
`;
  await writeFile(join(directory, "local.svg"), svg);
  await writeFile(join(directory, "index.md"), markdown);
  await writeFile(
    join(directory, "summaries.json"),
    `${JSON.stringify(layoutSummaries ?? summaries, null, 2)}\n`,
  );
}
