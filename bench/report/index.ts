// SPDX-License-Identifier: BSD-2-Clause

import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { median } from "../statistics.ts";
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
  if (!/^\d+$/.test(String(dataset.host.affinity)))
    warnings.push(
      `CPU affinity: ${formatCell(dataset.host.affinity)}; recommended one logical CPU`,
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
  for (const c of dataset.manifests.get("default")!.cases) {
    const times = (build: string) =>
      dataset.samples
        .filter((s) => s.build === build && s.case_id === caseId(c) && s.variant === "std-stable")
        .map((s) => s.elapsed_ns / s.batch);
    const d = times("default"),
      p = times("portable-simd");
    if (p.length && d.length && Math.abs(median(p) / median(d) - 1) > 0.05)
      warnings.push(
        `${caseId(c)}: slice::sort anchor drift ${(100 * (median(p) / median(d) - 1)).toFixed(1)}% between builds`,
      );
  }
  return warnings;
}
function renderResults(dataset: Dataset, summaries: Map<string, Summary>): string {
  let output =
    "## Full results\n\nTimes include each sort's scratch allocation. Ratios divide the variant's time by the mean of Tessera's paired samples in the same block. Bounds are per-block minima and maxima. The A/A noise range spans both Tessera samples divided by their block mean, across all blocks. Faster/slower describes the variant relative to Tessera; it requires 8 of 9 blocks to agree and the median to fall outside that range. The tessera-aa row separately reports A2 / A1.\n\n";
  const variants = [
    ...new Set([...dataset.manifests.values()].flatMap((m) => m.variants.map((v) => v.name))),
    "tessera-aa",
  ];
  output +=
    "Each cell gives ratio, ns / element, per-block ratio bounds, verdict and peak extra heap bytes. A dash means the variant is outside that build or task. Full-precision values are in [summaries.json](summaries.json).\n\n";
  for (const ty of types) {
    const rows: string[][] = [];
    for (const [build, manifest] of dataset.manifests) {
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
            return `${s.ratio.toFixed(4)}×<br>${s.ns.toFixed(3)} ns<br>[${s.low.toFixed(4)}, ${s.high.toFixed(4)}]<br>${s.verdict}<br>${memory ? `${memory.peak_extra_bytes} B` : "heap unmeasured"}`;
          }),
        ]);
      }
    }
    if (rows.length)
      output += `<details>\n<summary>${ty}</summary>\n\n${renderTable(["Shape", "Length", "Build", "A/A range", ...variants], rows)}\n</details>\n\n`;
  }
  return output;
}
function renderCapabilities(dataset: Dataset) {
  const manifest = dataset.manifests.get("default")!;
  const rows = manifest.variants.map((v) => [
    v.name,
    v.stable ? "stable" : "unstable",
    ...types.map((ty) =>
      manifest.cases.some((c) => c.ty === ty && c.variants.includes(v.name)) ? "yes" : "—",
    ),
  ]);
  return `## Capabilities\n\n${renderTable(["Variant", "Stability", ...types], rows)}\nUnstable sorts on records are listed as a separate task: their permutation check allows equal keys to change order. A dash means the adapter does not support the task.\n\n`;
}
function renderProvenance(dataset: Dataset) {
  let out = `## Measurement environment\n\n${renderTable(["Condition", "Recorded value"], Object.entries(dataset.host))}\n`;
  for (const [build, manifest] of dataset.manifests) {
    out += `### ${build}\n\nCommit \`${manifest.commit}\` (${manifest.dirty ? "modified tree" : "clean tree"}), target \`${manifest.target}\`, profile \`${manifest.profile}\`, optimization level \`${manifest.opt_level}\`, debug \`${manifest.debug}\`. Source SHA-256: \`${manifest.source_sha256}\`. Executable SHA-256: \`${manifest.binary_sha256}\`.\n\n`;
    out += `\`\`\`text\n${manifest.rustc}\n\`\`\`\n\n`;
    for (const compiler of ["cc", "cxx"] as const) {
      if (manifest[compiler])
        out += `${compiler}:\n\n\`\`\`text\n${manifest[compiler]}\n\`\`\`\n\n`;
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
        ["radsort", "rdst", "glidesort", "voracious_radix_sort", "tessera-sort"].includes(name)
        ? [[name, version]]
        : [];
    });
    out += `${renderTable(["Rust crate", "Version"], packages)}\n`;
  }
  return out;
}
export async function writeReport(directories: string[], partial = false): Promise<void> {
  const dataset = await loadRun(directories, partial),
    summaries = summarize(dataset),
    charts = renderFigures(dataset, summaries);
  const out = join(directories[0], "report");
  await mkdir(out, { recursive: true });
  let markdown = `# Sorting comparison\n\n${partial ? "Partial suite; only completed groups are shown." : `${dataset.manifests.get("default")!.suite} suite, version 1.`} Every value is regenerated from the accompanying manifests, host fingerprint, blocks and memory samples.\n\n`;
  markdown +=
    "Nine blocks reuse each case's seed-one input. Their range measures execution variability on that input; it does not measure variability across random inputs. Calibration can choose different batch sizes for different variants, changing the prepared-buffer working set and cache behavior.\n\n";
  markdown += renderProvenance(dataset);
  const conditions = collectWarnings(dataset);
  if (conditions.length)
    markdown += `## Observed conditions\n\n${conditions.map((w) => `- ${w}`).join("\n")}\n\n`;
  markdown += renderCapabilities(dataset);
  for (const chart of charts) {
    await writeFile(join(out, `${chart.name}.svg`), chart.svg);
    markdown += `## ${chart.title}\n\n![${chart.title}](${chart.name}.svg)\n\n<details>\n<summary>Plotted values</summary>\n\n${chart.table}\n</details>\n\n`;
  }
  markdown += renderResults(dataset, summaries);
  markdown +=
    "## Memory accounting\n\nThe memory executable measures one sort after allocating its input and verification reference. Rust-only builds count requested allocation sizes. Native builds count allocator usable sizes at the wrapped C allocation functions and C++ new/delete; allocator rounding is included. Fixed stack storage is unmeasured. Input buffers and owned payloads are excluded.\n";
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
  console.log(
    `Report: ${join(out, "index.md")} (${summaries.size} summaries, ${charts.length} figures)`,
  );
}
