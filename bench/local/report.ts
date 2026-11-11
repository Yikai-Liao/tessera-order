// SPDX-License-Identifier: BSD-2-Clause

import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { rawCaseParts } from "../catalog.ts";
import { renderRows } from "../plot/rows.ts";
import { resultSummary } from "../summary.ts";
import { type LayoutRun, summarizeLayouts } from "./layout.ts";
import { type LocalRun, summarizeLocal } from "./model.ts";

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
      title: `Layout times · ${Object.values(rawCaseParts(summary.id)).filter(Boolean).join(" · ")}`,
      guide: "Median time per sort · lines: observed sample ranges, not confidence intervals",
      columns: [`Median (${label})`, `Observed range (${label})`],
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
        note: "baseline and current: medians of their recorded time observations",
      },
      axis: ["← less time", `${label} per sort →`],
    });
    await writeFile(join(directory, filename), svg);
    figures.push(`![Absolute layout times for ${summary.id}](${filename})`);
  }
  return figures.join("\n\n");
}

/** Write a local comparison's table, provenance and process-level control chart. */
export async function renderLocalReport(directory: string, input?: LocalRun | LayoutRun) {
  const result =
    input ??
    (JSON.parse(await readFile(join(directory, "local.json"), "utf8")) as LocalRun | LayoutRun);
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
  const range = (low: number | undefined, high: number | undefined) =>
    low === undefined || high === undefined ? "—" : `${low.toFixed(4)}–${high.toFixed(4)}`;
  const groups = runs.map(({ title, run }) => ({ title, summaries: summarizeLocal(run) }));
  const summaries = groups.flatMap(({ summaries }) => summaries);
  const ratios = summaries.flatMap((s) => [s.low, s.high]);
  const reach = 2 ** Math.ceil(Math.log2(Math.max(2, ...ratios.map((v) => Math.max(v, 1 / v)))));
  const ticks = [];
  for (let k = -Math.log2(reach); k <= Math.log2(reach); k++)
    ticks.push({ value: 2 ** k, label: `${2 ** k}×` });
  const svg = renderRows({
    title: "Working tree against commit",
    guide: "Adjacent A/B sample ratios · lower is better · 99% median interval",
    columns: ["Time ratio", "Median interval"],
    groups: groups.map(({ title, summaries }) => ({
      title,
      rows: summaries.map((s) => ({
        name: {
          ...rawCaseParts(s.id),
          tag: "",
        },
        marks: [{ value: s.ratio, tone: "primary" }],
        span: [s.low, s.high],
        cells: [`${s.ratio.toFixed(4)}×`, range(s.low, s.high)],
        strong: true,
      })),
    })),
    scale: { low: 1 / reach, high: reach, ticks, origin: 1, clamp: false },
    legend: {
      note: "dot: median · line: median interval",
    },
    axis: ["← less time", "more time →"],
  });
  const table = [
    `| Layout | Case | Current / baseline | Median interval | Screen |`,
    "| --- | --- | --- | --- | --- |",
    ...groups.flatMap(({ title, summaries }) =>
      summaries.map(
        (s) =>
          `| ${title} | ${s.id} | ${s.ratio.toFixed(4)} | ${s.low.toFixed(4)}–${s.high.toFixed(4)} | ${s.verdict} |`,
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

Adjacent A/B samples alternate AB/BA order. The median ratio and its distribution-free 99% interval describe the paired samples; an alert requires the entire interval outside 0.95–1.05. Coverage assumes independent samples and does not include code-placement effects. Input restoration, verification and IPC are outside the sort clock. Raw pairs are retained; this is a regression screen, not proof of source-change causality.

${
  layoutSummaries
    ? `Each fixed seed shuffles LLD’s .text.* input sections for both revisions. Every seed retains a complete comparison under its recorded protocol. Equal seeds do not imply matching addresses between revisions. The cross-layout median and min/max describe only the observed sample, with no pooled regression verdict or confidence interval. Spreads are max/min minus one across each revision's per-layout median times; they include process and host variation. Link maps are retained as layout-<seed>-<revision>.map. Final rustc arguments are recorded separately from Cargo's dependency flags.

${layoutTable}

${layoutFigures}
`
    : ""
}

![Working tree against commit](local.svg)

${table}

## Provenance

\`\`\`json
${JSON.stringify(result.schema === 2 ? { ...result, layouts: result.layouts.map(({ seed, run }) => ({ seed, run: { ...run, samples: undefined } })) } : { ...run, samples: undefined }, null, 2)}
\`\`\`
`;
  await writeFile(join(directory, "local.svg"), svg);
  await writeFile(join(directory, "index.md"), markdown);
  await writeFile(
    join(directory, "summaries.json"),
    `${JSON.stringify(layoutSummaries ?? summaries, null, 2)}\n`,
  );
  return {
    kind: result.schema === 2 ? "layout-comparison" : "revision-comparison",
    protocol: "paired-samples-v2",
    ratio: "current / baseline",
    range: "99% median interval",
    baseline: run.baseline,
    head: run.head,
    dirty: run.dirty,
    affinity: run.host.affinity,
    ...(result.schema === 2
      ? { layouts: groups.map(({ title, summaries }) => ({ title, ...resultSummary(summaries) })) }
      : resultSummary(summaries)),
  };
}
