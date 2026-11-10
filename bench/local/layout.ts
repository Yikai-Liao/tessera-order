// SPDX-License-Identifier: BSD-2-Clause

import { median } from "../statistics.ts";
import { type LocalRun, summarizeLocal } from "./model.ts";

export const LAYOUT_CONTROLS = ["cgu1", "aligned", "engine-first"] as const;
export type LayoutControl = (typeof LAYOUT_CONTROLS)[number];

/** Select engine-owned functions, keeping generic mentions in harness names out. */
export function engineOrderingSymbols(raw: string, demangled: string) {
  const symbols = raw
      .trim()
      .split("\n")
      .map((line) => line.trimEnd()),
    names = demangled
      .trim()
      .split("\n")
      .map((line) => line.trimEnd());
  if (symbols.length !== names.length) throw new Error("nm symbol views differ");
  return symbols
    .flatMap((line, index) => {
      const symbol = /^(\S+) ([tT]) ([a-f\d]+)(?: ([a-f\d]+))?$/i.exec(line);
      if (!symbol) return [];
      const name = /^(.+) ([tT]) ([a-f\d]+)(?: ([a-f\d]+))?$/i.exec(names[index]);
      if (!name || symbol.slice(2).join() !== name.slice(2).join())
        throw new Error("nm symbol views differ");
      return /^(?:tessera_sort::|<tessera_sort::|<[^<>]+ as tessera_sort::)/.test(name[1])
        ? [symbol[1]]
        : [];
    })
    .sort();
}

export interface LayoutRun {
  schema: 2;
  linker: { driver: string; version: string };
  control?: LayoutControl;
  layouts: { seed: number; run: LocalRun }[];
}

/** Require distinct reproducible LLD seeds; zero asks LLD for randomness. */
export function parseLayoutSeeds(value: string): number[] {
  const parts = value.split(",").map((part) => part.trim());
  const seeds = parts.map(Number);
  if (
    parts.some((part) => !/^\d+$/.test(part)) ||
    seeds.some((seed) => !Number.isSafeInteger(seed) || seed < 1 || seed > 0xffff_ffff) ||
    new Set(seeds).size !== seeds.length
  )
    throw new Error("expected distinct integer layout seeds in 1..4294967295 (0 is random)");
  return seeds;
}

/** Reject incomparable builds and ineffective seeds before spending measurement time. */
export function validateLayoutBuilds(result: LayoutRun) {
  if (result.schema !== 2 || !result.layouts.length) throw new Error("Expected layout runs");
  parseLayoutSeeds(result.layouts.map(({ seed }) => seed).join(","));
  const first = result.layouts[0].run;
  for (const { run } of result.layouts) {
    for (const field of [
      "baseline",
      "head",
      "configuration",
      "rustc",
      "flags",
      "protocol",
    ] as const)
      if (run[field] !== first[field]) throw new Error(`Layout runs differ: ${field}`);
    if (JSON.stringify(run.source) !== JSON.stringify(first.source))
      throw new Error("Layout runs differ: sources");
    if (run.cases.join("\n") !== first.cases.join("\n"))
      throw new Error("Layout runs must select identical cases");
  }
  for (const role of ["baseline", "current"] as const) {
    const hashes = result.layouts.map(({ run }) => run.binaries[role]);
    if (new Set(hashes).size !== hashes.length)
      throw new Error(`Layout seeds produced identical ${role} executables`);
  }
}

/** Describe the observed layout sample without treating its range as a confidence interval. */
export function summarizeLayouts(result: LayoutRun) {
  validateLayoutBuilds(result);
  const first = result.layouts[0].run;
  const summaries = result.layouts.map(({ run }) => summarizeLocal(run));
  return first.cases.map((id, index) => {
    const layouts = result.layouts.map(({ seed, run }, layout) => {
      const paired = run.protocol === "paired-samples-v1" || run.protocol === "paired-samples-v2";
      const baseline = paired
        ? run.samples![id].map((s) => s.baseline)
        : run.blocks.map((block) => (block.baseline[id] + block.control[id]) / 2);
      const current = paired
        ? run.samples![id].map((s) => s.current)
        : run.blocks.map((block) => block.current[id]);
      return {
        seed,
        ...summaries[layout][index],
        baselineNs: median(baseline),
        baselineLowNs: Math.min(...baseline),
        baselineHighNs: Math.max(...baseline),
        currentNs: median(current),
        currentLowNs: Math.min(...current),
        currentHighNs: Math.max(...current),
      };
    });
    const ratios = layouts.map((layout) => layout.ratio);
    const baseline = layouts.map((layout) => layout.baselineNs);
    const current = layouts.map((layout) => layout.currentNs);
    return {
      id,
      ratio: median(ratios),
      low: Math.min(...ratios),
      high: Math.max(...ratios),
      baselineSpread: Math.max(...baseline) / Math.min(...baseline) - 1,
      currentSpread: Math.max(...current) / Math.min(...current) - 1,
      layouts,
    };
  });
}
