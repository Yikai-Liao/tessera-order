// SPDX-License-Identifier: BSD-2-Clause

import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { parseCriterionId, revisionSampling } from "../../catalog.ts";
import {
  type LocalRun,
  type PreparedPair,
  revisionFeatures,
  shardCases,
} from "../../local/model.ts";
import { loadPlan, type Plan } from "../../plan.ts";
import { continuousShards } from "../paired.ts";
import { baseHash, hash } from "./report.ts";

const allCases = [48, 49, 1000, 65536, 1000000]
  .flatMap((len) =>
    ["u32", "u64", "row_u64"].flatMap((ty) =>
      ["api", "par-4"].map((mode) => `rust/${ty}/random/${len}/s1/${mode}`),
    ),
  )
  .sort();
export const PAIRED_SHARDS = continuousShards(allCases);

/** Synthetic blocks exercise protocol validation, not measured performance. */
export function pairedRun(
  index = 1,
  ratio = 1.2,
  inventory = allCases,
  features = ["operations", "rayon"],
): LocalRun {
  const count = continuousShards(inventory);
  const cases = shardCases(inventory, index, count);
  const build = {
    rustc: "rustc fixture",
    target: "x86_64-unknown-linux-gnu",
    profile: "release",
    opt_level: "3",
    debug: "false",
    cargo_encoded_rustflags: "fixture",
    features: features
      .map((feature) => `CARGO_FEATURE_${feature.toUpperCase().replaceAll("-", "_")}`)
      .sort(),
  };
  return {
    schema: 1,
    protocol: "paired-samples-v2",
    head: hash,
    baseline: baseHash,
    dirty: false,
    source: {
      baseline: "b".repeat(64),
      current: "a".repeat(64),
      harness: "c".repeat(64),
      catalog: "d".repeat(64),
    },
    binaries: { baseline: "e".repeat(64), current: "f".repeat(64) },
    binaryPaths: { baseline: "binaries/baseline/worker", current: "binaries/current/worker" },
    builds: { baseline: build, current: build },
    rustc: build.rustc,
    flags: build.cargo_encoded_rustflags,
    configuration: "portable-simd",
    host: {
      platform: "linux",
      release: "fixture",
      arch: "x64",
      cpu: "fixture",
      affinity: "0,1,2,3",
      aslr: "disabled",
    },
    cases,
    shard: { index, count, allCases: inventory },
    sampleCounts: Object.fromEntries(
      cases.map((id) => [id, revisionSampling(parseCriterionId(id).len).samples]),
    ),
    samples: Object.fromEntries(
      cases.map((id) => [
        id,
        Array.from({ length: revisionSampling(parseCriterionId(id).len).samples }, () => ({
          baseline: 100,
          current: 100 * ratio,
        })),
      ]),
    ),
  };
}

/** Write the exact directory layout produced by download-artifact. */
export async function writePaired(
  directory: string,
  ratio = 1.2,
  selection?: Plan,
  inventory = allCases,
) {
  const plan = selection ?? (await loadPlan("continuous"));
  const runs: LocalRun[] = [];
  for (let index = 1; index <= continuousShards(inventory); index++) {
    const out = join(directory, `paired-${index}`);
    await mkdir(out, { recursive: true });
    const run = pairedRun(index, ratio, inventory, revisionFeatures(plan));
    await writeFile(join(out, "local.json"), JSON.stringify(run));
    await writeFile(join(out, "plan.json"), JSON.stringify(plan));
    runs.push(run);
  }
  const { shard, samples: _samples, sampleCounts: _counts, host: _host, ...pair } = runs[0];
  const prepared: PreparedPair = {
    ...pair,
    cases: shard!.allCases,
    kind: "prepared-revisions",
    plan,
  };
  await mkdir(join(directory, "prepared"), { recursive: true });
  await writeFile(join(directory, "prepared", "prepared.json"), JSON.stringify(prepared));
  return runs;
}
