// SPDX-License-Identifier: BSD-2-Clause

import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { renderRows } from "../plot/rows.ts";
import { type LocalRun, summarizeLocal } from "./model.ts";

/** Write a local comparison's table, provenance and process-level control chart. */
export async function renderLocalReport(directory: string, input?: LocalRun) {
  const run =
    input ?? (JSON.parse(await readFile(join(directory, "local.json"), "utf8")) as LocalRun);
  if (run.schema !== 1) throw new Error("Unsupported local result schema");
  const summaries = summarizeLocal(run);
  const ratios = summaries.flatMap((s) => [s.low, s.high, s.aaLow, s.aaHigh]);
  const reach = 2 ** Math.ceil(Math.log2(Math.max(2, ...ratios.map((v) => Math.max(v, 1 / v)))));
  const ticks = [];
  for (let k = -Math.log2(reach); k <= Math.log2(reach); k++)
    ticks.push({ value: 2 ** k, label: `${2 ** k}×` });
  const svg = renderRows({
    title: "Working tree against commit",
    guide: "Current / baseline run time · lower is better · nine process-level blocks",
    columns: ["Time ratio", "A/A range"],
    groups: [
      {
        title: run.configuration,
        rows: summaries.map((s) => ({
          name: {
            input: s.id.split("/").slice(1, 3).join(" · "),
            sub: s.id.split("/").slice(3).join(" · "),
            tag: "",
          },
          marks: [{ value: s.ratio, tone: "tessera" }],
          span: [s.low, s.high],
          cells: [`${s.ratio.toFixed(4)}×`, `${s.aaLow.toFixed(4)}–${s.aaHigh.toFixed(4)}`],
          strong: true,
        })),
      },
    ],
    scale: { low: 1 / reach, high: reach, ticks, origin: 1, clamp: false },
    legend: { note: "dot: median · line: block range" },
    axis: ["← less time", "more time →"],
  });
  const table = [
    "| Case | Current / baseline | Block range | A/A range | Screen |",
    "| --- | --- | --- | --- | --- |",
    ...summaries.map(
      (s) =>
        `| ${s.id} | ${s.ratio.toFixed(4)} | ${s.low.toFixed(4)}–${s.high.toFixed(4)} | ${s.aaLow.toFixed(4)}–${s.aaHigh.toFixed(4)} | ${s.verdict} |`,
    ),
  ].join("\n");
  const markdown = `# Local comparison

Baseline: \`${run.baseline}\`. Working-tree HEAD: \`${run.head}\` (${run.dirty ? "modified" : "clean"}).

This is a development screen, not a placement-controlled performance claim. Each process measures Criterion's mean time per sort. Each block runs the baseline executable twice and the current executable once; ratios use the paired baseline mean. A direction requires eight of nine blocks and a median outside the A/A range. All processes use the same input seed and current harness; they do not share an address space.

![Working tree against commit](local.svg)

${table}

## Provenance

\`\`\`json
${JSON.stringify({ ...run, blocks: undefined }, null, 2)}
\`\`\`
`;
  await writeFile(join(directory, "local.svg"), svg);
  await writeFile(join(directory, "index.md"), markdown);
  await writeFile(join(directory, "summaries.json"), `${JSON.stringify(summaries, null, 2)}\n`);
  console.log(`Report: ${join(directory, "index.md")}`);
}
