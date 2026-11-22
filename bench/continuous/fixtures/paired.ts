// SPDX-License-Identifier: BSD-2-Clause

import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { continuous, parseCriterionId, revisionSampling } from "../../catalog.ts";
import { type LocalRun, type PreparedPair, shardCases } from "../../local/model.ts";
import { loadPlan } from "../../plan.ts";
import { CONTINUOUS_SHARDS } from "../paired.ts";
import { baseHash, hash } from "./report.ts";

const defaultCases = [48, 49, 1000, 65536, 1000000]
  .flatMap((len) =>
    ["serial", "t4"].map((execution) => `rust/sort/u64/random/${len}/s1/${execution}`),
  )
  .sort();

/** Synthetic blocks exercise protocol validation, not measured performance. */
export function pairedRun(
  index = 1,
  ratio = 1.2,
  allCases = defaultCases,
  count = CONTINUOUS_SHARDS,
): LocalRun {
  const cases = shardCases(allCases, index, count);
  const build = {
    rustc: "rustc fixture",
    target: "x86_64-unknown-linux-gnu",
    profile: "release",
    opt_level: "3",
    debug: "false",
    cargo_encoded_rustflags: "fixture",
    features: continuous.features
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
    shard: { index, count, allCases },
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
  selection?: Pick<PreparedPair, "cases" | "plan">,
  count = CONTINUOUS_SHARDS,
) {
  const plan = selection?.plan ?? (await loadPlan("continuous"));
  const runs: LocalRun[] = [];
  for (let index = 1; index <= count; index++) {
    const out = join(directory, `paired-${index}`);
    await mkdir(out, { recursive: true });
    const run = pairedRun(index, ratio, selection?.cases, count);
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
