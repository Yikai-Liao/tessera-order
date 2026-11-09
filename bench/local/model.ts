// SPDX-License-Identifier: BSD-2-Clause

import { summarizePaired } from "../statistics.ts";
import type { BuildProvenance } from "../workspace.ts";

export interface LocalBlock {
  baseline: Record<string, number>;
  control: Record<string, number>;
  current: Record<string, number>;
  elapsedMs?: { baseline: number; control: number; current: number };
}

export interface LocalRun {
  schema: 1;
  baseline: string;
  head: string;
  dirty: boolean;
  source: { baseline: string; current: string; harness: string; catalog: string };
  binaries: { baseline: string; current: string };
  builds?: { baseline: BuildProvenance; current: BuildProvenance };
  rustc: string;
  flags: string;
  configuration: string;
  host: { platform: string; release: string; arch: string; cpu: string; affinity: string };
  cases: string[];
  blocks: LocalBlock[];
}

export interface LocalSummary {
  id: string;
  ratio: number;
  low: number;
  high: number;
  aaLow: number;
  aaHigh: number;
  verdict: "faster" | "slower" | "within noise";
}

/** Reject missing, extra or invalid case observations in a process block. */
export function validateBlock(cases: string[], block: LocalBlock) {
  for (const values of [block.baseline, block.control, block.current]) {
    if (
      Object.keys(values).sort().join("\n") !== [...cases].sort().join("\n") ||
      Object.values(values).some((v) => !Number.isFinite(v) || v <= 0)
    )
      throw new Error("Block results do not match the selected cases");
  }
}

/** Summarize nine process-level blocks with their paired A/A controls. */
export function summarizeLocal(run: Pick<LocalRun, "cases" | "blocks">): LocalSummary[] {
  if (run.blocks.length !== 9 || !run.cases.length)
    throw new Error("Expected nine complete blocks");
  for (const block of run.blocks) validateBlock(run.cases, block);
  return run.cases.map((id) => ({
    id,
    ...summarizePaired(
      run.blocks.map((block) => ({
        first: block.baseline[id],
        second: block.control[id],
        value: block.current[id],
      })),
    ),
  }));
}
