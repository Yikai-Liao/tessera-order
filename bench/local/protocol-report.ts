// SPDX-License-Identifier: BSD-2-Clause

import { writeFile } from "node:fs/promises";
import { join } from "node:path";
import { type ProtocolRun, summarizeProtocols } from "./protocol.ts";

/** Report observed alert counts and actual wall costs for a prespecified local pilot. */
export async function renderProtocolReport(directory: string, run: ProtocolRun) {
  const summary = summarizeProtocols(run);
  const percent = (ratio: number) => `${((ratio - 1) * 100).toFixed(2)}%`;
  const table = [
    "| Case | Expected | Strategy | Matches / 9 | Faster alerts | Slower alerts | Within ±5% | Median change | Process A/A range |",
    "| --- | --- | --- | --- | --- | --- | --- | --- | --- |",
    ...summary.cases.flatMap((row) =>
      (["single", "multiple"] as const).map((strategy) => {
        const value = row[strategy];
        return `| ${row.id} | ${row.expected} | ${strategy === "single" ? "One full-window layout" : "Median of three divided-window layouts"} | ${value.matchesExpectation}/9 | ${value.faster} | ${value.slower} | ${value.withinLimits} | ${percent(value.ratio)} | ${percent(value.aaLow)}–${percent(value.aaHigh)} |`;
      }),
    ),
  ].join("\n");
  const trials = summary.cases
    .map((row) =>
      [
        `### ${row.id}`,
        "",
        "| Trial | Single change | Single screen | Three-layout change | Three-layout screen | Single process cost (s) | Three-layout process cost (s) |",
        "| --- | --- | --- | --- | --- | --- | --- |",
        ...row.single.observations.map((single, index) => {
          const multiple = row.multiple.observations[index];
          const time = summary.jobCost.trials[index];
          return `| ${index + 1} | ${percent(single.ratio)} | ${single.direction} | ${percent(multiple.ratio)} | ${multiple.direction} | ${(time.singleMs / 1000).toFixed(3)} | ${(time.multipleMs / 1000).toFixed(3)} |`;
        }),
      ].join("\n"),
    )
    .join("\n\n");
  const markdown = `# Local protocol study

Prespecified expectation: **${run.expected}**. Baseline: \`${run.provenance.baseline}\`; working-tree HEAD: \`${run.provenance.head}\`.

This pilot compares nine pairs of strategy trials against the current candidate-only process budget. Each trial has six distinct layouts, and no seed is reused in another trial. Baseline/current seeds differ even in a same-source experiment. The single-layout arm rotates through the three revision pairs with full Criterion settings; the three-layout arm uses all three pairs with warm-up and measurement windows divided by six and sample counts rounded upward, retaining Criterion's ten-sample minimum. This allocates the candidate-only window across three baseline and three current processes. All seed assignments and effective per-case settings are retained. Both arms also repeat each baseline for diagnostic A/A controls. Arm order alternates between trials; process and layout order rotate. All builds finish before timing, and both arms use exactly the same binaries for shared seeds.

Each trial screens current / baseline against the existing fixed 0.95–1.05 limits. The three-layout score divides the median of current layout times by the median of baseline layout times. Diagnostic control processes never enter either score: only processes charged to the candidate job or its cached baseline supply observations. Matches means within limits for a same-source or unaffected control, or an alert in the prespecified direction for changed source. A/A ranges compare repeated processes of the same baseline executable; they do not define the alert limits. A historical change's expected direction is a hypothesis, not proof that its effect exceeds 5% on this host.

${table}

Three-layout / single-layout process-cost ratio: median **${summary.jobCost.ratio.toFixed(4)}**, observed range **${summary.jobCost.low.toFixed(4)}–${summary.jobCost.high.toFixed(4)}**. The prespecified budget gate is a median paired ratio at most **1.0000**: **${summary.jobCost.fitsObservedBudget ? "fits the observed process budget" : "exceeds the observed process budget"}**. Single charges the current process, treating its baseline as cached; three-layout charges three baseline and three current processes. Each charged process includes startup, input preparation, verification, Criterion warm-up, sampling, analysis and result collection. Per-case tables repeat these whole-selection costs, not per-case estimates. This is measured local process cost, not an end-to-end Bencher job measurement.

The complete diagnostic arm wall-time ratio, including all extra A/A processes and arm-specific setup/collection, has median **${summary.wallTime.ratio.toFixed(4)}** and range **${summary.wallTime.low.toFixed(4)}–${summary.wallTime.high.toFixed(4)}**. Both arms' diagnostic costs are retained separately in summaries.json. Build time and case listing are outside measurement. Dividing windows does not guarantee the process-cost gate: minimum samples, repeated setup and analysis can exceed it.

These are observed counts from nine held-out layout cohorts, not a population false-positive rate or a confidence interval. A historical source pair tests only its selected change and inputs; it does not establish sensitivity to arbitrary 5% or 10% regressions. Both arms run on one local host in the same session; this does not reproduce historical-machine drift, the continuous worker concurrency or a deployed Bencher protocol. No Testbed, continuous setting or alert rule is changed.

## Trials

${trials}

## Provenance

\`\`\`json
${JSON.stringify({ ...run, trials: run.trials.map(({ pairs }) => ({ pairs })) }, null, 2)}
\`\`\`
`;
  await writeFile(join(directory, "index.md"), markdown);
  await writeFile(join(directory, "summaries.json"), `${JSON.stringify(summary, null, 2)}\n`);
}
