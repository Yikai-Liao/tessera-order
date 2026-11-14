// SPDX-License-Identifier: BSD-2-Clause

import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { catalog, operationLabel } from "../catalog.ts";
import { median, pairedBudget } from "../statistics.ts";
import { resultSummary } from "../summary.ts";
import { renderFigures } from "./charts.ts";
import { loadRun } from "./load.ts";
import { caseId, type Dataset, measurementId, type Summary, summarize, types } from "./model.ts";

function formatCell(value: unknown) {
  return String(value ?? "unavailable")
    .replaceAll("|", "\\|")
    .replaceAll("\n", " ");
}
function renderTable(headers: string[], rows: unknown[][]) {
  return `| ${headers.map(formatCell).join(" | ")} |\n| ${headers.map(() => "---").join(" | ")} |\n${rows.map((r) => `| ${r.map(formatCell).join(" | ")} |`).join("\n")}\n`;
}
export function collectWarnings(dataset: Dataset): string[] {
  const warnings: string[] = [];
  for (const [field, expected] of [
    ["governor", "performance"],
    ["turbo", "off"],
    ["smt", "0"],
  ]) {
    if (dataset.host[field] !== expected)
      warnings.push(`${field}: ${formatCell(dataset.host[field])}; recommended ${expected}`);
  }
  const parallel = dataset.manifest.variants.some((v) => v.threads);
  if (!parallel && !/^\d+$/.test(String(dataset.host.affinity)))
    warnings.push(
      `CPU affinity: ${formatCell(dataset.host.affinity)}; recommended one logical CPU`,
    );
  if (parallel)
    warnings.push(
      `Parallel run: affinity ${formatCell(dataset.host.affinity)} is shared by caller-owned pools; verify it covers the requested physical cores. Worker count is not physical-core count.`,
    );
  const observations = dataset.observations;
  const signals: [string, (o: Dataset["observations"][number]) => number | null, number][] = [
    ["load", (o) => (o.load ? Number(o.load.split(" ")[0]) : null), 1],
    ["frequency", (o) => (o.frequency_khz ? Number(o.frequency_khz) : null), 0],
    [
      "temperature",
      (o) =>
        o.temperatures_millidegrees?.length ? Math.max(...o.temperatures_millidegrees) : null,
      10000,
    ],
  ];
  for (const [name, read, absolute] of signals) {
    const values = observations
      .map(read)
      .filter((n): n is number => n !== null && Number.isFinite(n));
    if (!values.length) continue;
    const center = median(values),
      deviation = Math.max(absolute, Math.abs(center) * 0.2);
    for (const o of observations) {
      const value = read(o);
      if (value !== null && Math.abs(value - center) > deviation)
        warnings.push(
          `${o.build} ${o.group} ${o.phase}: ${name} ${value} differs from run median ${center}`,
        );
    }
  }
  return warnings;
}
function renderResults(dataset: Dataset, summaries: Map<string, Summary>): string {
  let output =
    "## Full results\n\nTimes include each complete call's scratch and mandatory output allocation. Ratios divide the variant's time by the mean of Tessera's paired samples in the same block. Bounds are per-block minima and maxima. The A/A noise range spans both Tessera samples divided by their block mean, across all blocks. Faster/slower describes the variant relative to Tessera; it requires the declared budget's direction agreement and a median outside that range (see Measurement budget). A short screen is inconclusive when either condition fails. The tessera-aa row separately reports A2 / A1.\n\n";
  const variants = [...new Set(dataset.manifest.variants.map((v) => v.name)), "tessera-aa"];
  output +=
    "Each cell gives ratio, ns / input record, per-block ratio bounds, verdict and peak extra heap bytes. A dash means the variant is outside that build or task. Full-precision values are in [summaries.json](summaries.json).\n\n";
  for (const ty of new Set([...types, ...dataset.manifest.cases.map((c) => c.ty)])) {
    const rows: string[][] = [];
    const manifest = dataset.manifest;
    const build = "default";
    for (const c of manifest.cases.filter((c) => c.ty === ty)) {
      const baseline = summaries.get(measurementId(build, caseId(c), "tessera"));
      if (!baseline) continue;
      rows.push([
        c.distribution,
        c.len.toLocaleString("en-US"),
        build,
        `${baseline.aaLow.toFixed(4)}–${baseline.aaHigh.toFixed(4)}`,
        ...variants.map((variant) => {
          const s = summaries.get(measurementId(build, caseId(c), variant));
          if (!s) return "—";
          const memory = dataset.memory.find(
            (m) =>
              measurementId(m.build, m.case_id, m.variant) ===
              measurementId(build, caseId(c), variant),
          );
          const heap = memory
            ? `${memory.peak_extra_bytes} B peak${memory.output_bytes !== undefined ? `<br>${memory.output_bytes} B output + ${memory.peak_extra_bytes - memory.output_bytes} B workspace peak` : ""}`
            : "heap unmeasured";
          return `${s.ratio.toFixed(4)}×<br>${s.ns.toFixed(3)} ns<br>[${s.low.toFixed(4)}, ${s.high.toFixed(4)}]<br>${s.verdict}<br>${heap}`;
        }),
      ]);
    }
    if (rows.length)
      output += `<details>\n<summary>${ty}</summary>\n\n${renderTable(["Shape", "Length", "Build", "A/A range", ...variants], rows)}\n</details>\n\n`;
  }
  return output;
}
function renderCapabilities(dataset: Dataset) {
  const manifest = dataset.manifest;
  const rows = manifest.variants.map((v) => [
    v.name,
    v.stable ? "stable" : "unstable",
    v.threads ?? 0,
    ...types.map((ty) =>
      manifest.cases.some((c) => c.ty === ty && c.variants.includes(v.name)) ? "yes" : "—",
    ),
  ]);
  let out = `## Capabilities\n\n${renderTable(["Variant", "Sort contract", "Pool workers", ...types], rows)}\n`;
  if (manifest.suite_version === 1)
    return (
      out +
      "Unstable sorts on records are listed as a separate task: their permutation check allows equal keys to change order. A dash means the adapter does not support the task.\n\n"
    );
  out +=
    "Stable routes require exact stable ranked membership. Standard-library unstable partial routes instead require the same ranked keys with arbitrary equal-key members, including on payload records; these are weaker-contract tasks, not equivalent stable selectors. Standard unstable segmented sorting remains scalar-only, and standard unstable argsort explicitly breaks ties by original index. Each admitted route or omission is retained below.\n\n";
  const routes = new Map<string, string[]>();
  for (const c of manifest.cases) {
    for (const [name, route] of Object.entries(c.implementations ?? {}))
      routes.set(`${c.ty}/${name}`, [c.ty, name, route]);
    for (const omitted of c.omitted ?? [])
      routes.set(`${c.ty}/${omitted.variant}`, [
        c.ty,
        omitted.variant,
        `omitted: ${omitted.reason}`,
      ]);
  }
  return (
    out +
    renderTable(["Input type", "Variant", "Complete call / omission"], [...routes.values()]) +
    "\n"
  );
}
function renderProvenance(dataset: Dataset) {
  let out = `## Measurement environment\n\n${renderTable(["Condition", "Recorded value"], Object.entries(dataset.host))}\n`;
  const manifest = dataset.manifest;
  const build = "default";
  out += `### ${build}\n\nCommit \`${manifest.commit}\` (${manifest.dirty ? "modified tree" : "clean tree"}), target \`${manifest.target}\`, profile \`${manifest.profile}\`, optimization level \`${manifest.opt_level}\`, debug \`${manifest.debug}\`. Source SHA-256: \`${manifest.source_sha256}\`. Executable SHA-256: \`${manifest.binary_sha256}\`.\n\n`;
  out += `\`\`\`text\n${manifest.rustc}\n\`\`\`\n\n`;
  for (const compiler of ["cc", "cxx"] as const) {
    if (manifest[compiler]) out += `${compiler}:\n\n\`\`\`text\n${manifest[compiler]}\n\`\`\`\n\n`;
  }
  out += `Features: ${manifest.features.map(formatCell).join(", ")}. Rust flags: \`${formatCell(manifest.cargo_encoded_rustflags).replaceAll("\u001f", " ")}\`.\n\n`;
  if (manifest.dispatch)
    out += `${renderTable(["SIMD sort", "Dispatch target"], Object.entries(manifest.dispatch))}\n`;
  if (manifest.native_sources)
    out += `<details>\n<summary>Native sources, commits and licenses</summary>\n\n\`\`\`toml\n${manifest.native_sources}\n\`\`\`\n\n</details>\n\n`;
  const packages = manifest.cargo_lock.split("[[package]]").flatMap((block) => {
    const name = block.match(/^name = "(.+)"$/m)?.[1],
      version = block.match(/^version = "(.+)"$/m)?.[1];
    return name &&
      version &&
      // `tessera-sort` names the crate in reports measured before the rename.
      [
        "radsort",
        "rdst",
        "glidesort",
        "voracious_radix_sort",
        "tessera-order",
        "tessera-sort",
      ].includes(name)
      ? [[name, version]]
      : [];
  });
  out += `${renderTable(["Rust crate", "Version"], packages)}\n`;
  return out;
}
export async function writeReport(directories: string[], partial = false) {
  const dataset = await loadRun(directories, partial),
    summaries = summarize(dataset),
    charts = dataset.manifest.suite_version === 1 ? renderFigures(dataset, summaries) : [];
  const out = join(directories[0], "report");
  await mkdir(out, { recursive: true });
  const operation = dataset.manifest.cases[0].operation;
  let markdown = `# ${operation ? operationLabel(operation) : "Sorting comparison"}\n\n${partial ? "Partial suite; only completed groups are shown." : `${dataset.manifest.suite} suite, version ${dataset.manifest.suite_version}.`} Every value is regenerated from the accompanying manifests, host fingerprint, blocks and memory samples.\n\n`;
  if (operation)
    markdown += `Operation: \`${JSON.stringify(operation)}\`. Times are complete-call latency divided by input length, not by requested ranks or output count. Stable full-sort reference rows perform more work and satisfy both stable and weaker key-ranked results; they are not native partial selectors. Ratios between stable and arbitrary-tie routes compare different guarantees, not the isolated cost of stability.\n\n`;
  markdown +=
    "All blocks reuse each case's seed-one input. Their range measures execution variability on that input; it does not measure variability across random inputs. Calibration can choose different batch sizes for different variants, changing the prepared-buffer working set and cache behavior.\n\n";
  markdown += "## Measurement budget\n\n";
  const { count, agreement } = pairedBudget(dataset.manifest.blocks);
  markdown += `**${count} blocks per case${count === catalog.pairedBlocks.short.count ? " (short screen)" : ""}**; ${agreement} of ${count} must agree.\n`;
  markdown +=
    "\nNo blocks are added automatically after an inconclusive result. These observed ranges are not confidence intervals.\n\n";
  markdown += renderProvenance(dataset);
  const revisions = dataset.manifest.revisions;
  if (revisions) {
    markdown +=
      "## Linked revision controls\n\nTwo named crates represent each revision, alongside the repeated primary-current A/A call. Ratios divide each placement by the current A/A mean: a slower baseline row indicates a current improvement. Compare both baseline copies and the second current copy, not only one favorable placement. Adapter entry offsets prove distinct call boundaries, not distinct internal sort bodies; inspect the retained executable before source attribution. This is a serial comparison; parallel revision process screens use the shared worker with explicit four-thread algorithms.\n\n";
    markdown += "```json\n" + JSON.stringify(revisions, null, 2) + "\n```\n\n";
  }
  const conditions = collectWarnings(dataset);
  if (conditions.length)
    markdown += `## Observed conditions\n\n${conditions.map((w) => `- ${w}`).join("\n")}\n\n`;
  markdown += renderCapabilities(dataset);
  if (dataset.manifest.variants.some((v) => v.threads))
    markdown +=
      "## Parallel measurement scope\n\nPools are started and warmed before timing and allocation counting; each timed call includes pool dispatch and sort scratch allocation. Peak heap bytes exclude pre-existing pool/runtime storage and thread stacks. All workers exit before the next adapter. Ratios use the serial Tessera A/A pair, not a separate parallel A/A pair. The inverse ratio is whole-call speedup over serial Tessera, not source-change attribution or parallel-only noise control.\n\n";
  if (!operation)
    markdown +=
      "## Figure scope\n\nStandard panels show fixed slices and aggregates, not every measured case. The 1M input panels include all selected distributions for each type, including the new skew, key-structure, disorder and finite-float cases. Record panels rank stable sorts only. Ranking and cross-type aggregate panels retain the ten baseline distributions for a common comparison; they do not summarize the whole expanded suite. The memory panel takes the worst across all selected 10M u64 distributions. The selected-input panel is a fallback used only when no standard panel can be drawn. All completed cases, including custom lengths absent from the figures, remain in [Full results](#full-results) and [summaries.json](summaries.json).\n\n";
  for (const chart of charts) {
    await writeFile(join(out, `${chart.name}.svg`), chart.svg);
    markdown += `## ${chart.title}\n\n![${chart.title}](${chart.name}.svg)\n\n<details>\n<summary>Plotted values</summary>\n\n${chart.table}\n</details>\n\n`;
  }
  markdown += renderResults(dataset, summaries);
  markdown +=
    "## Memory accounting\n\nThe memory executable measures one complete call after preparing its input and verification reference. Rust-only builds count requested allocation sizes. Native builds count allocator usable sizes at the wrapped C allocation functions and C++ new/delete; allocator rounding is included. Fixed stack storage is unmeasured. Input buffers and owned payloads are excluded.\n";
  if (operation)
    markdown +=
      "Mandatory output is allocated inside the measured call and included in peak extra heap bytes. Workspace peak is the recorded peak minus retained output bytes; argsort constructs its index result before sorting, so both coexist. In-place operations have zero output bytes. Multiple-selection normalization is inside the measured call. No sorting figures are generated for these result shapes.\n";
  await writeFile(join(out, "index.md"), markdown);
  await writeFile(
    join(out, "summaries.json"),
    `${JSON.stringify([...summaries.values()], null, 2)}\n`,
  );
  const bmf = Object.fromEntries(
    [...summaries.values()].map((s) => [
      `compare/${s.caseId}/${s.variant}/${s.build}`,
      {
        ratio: { value: s.ratio, lower_value: s.low, upper_value: s.high },
        latency: { value: s.ns },
      },
    ]),
  );
  await writeFile(join(directories[0], "bmf.json"), `${JSON.stringify(bmf, null, 2)}\n`);
  return {
    kind: "algorithm-comparison",
    partial,
    protocol: "paired-blocks-aa",
    blocks: count,
    ratio: "algorithm / paired Tessera mean",
    range: "observed block range",
    affinity: dataset.host.affinity ?? "unknown",
    // Ratios name the opponent: an opponent faster than Tessera is the actionable loss.
    ...resultSummary(
      [...summaries.values()]
        .filter((row) => row.variant !== "tessera" && row.variant !== "tessera-aa")
        .map((row) => ({ ...row, id: `${row.caseId}/${row.variant}` })),
      "faster",
    ),
  };
}
