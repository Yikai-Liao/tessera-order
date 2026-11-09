// SPDX-License-Identifier: BSD-2-Clause

import { copyFile, mkdir, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { CommandCancelledError, command, diagnostic } from "../../tools/process.ts";
import { parseCriterionId, VARIANTS } from "../catalog.ts";
import type { Plan } from "../plan.ts";
import { buildBenchmark, withWorkspace } from "../workspace.ts";

/** Validate an additive API listing without duplicating the plan's case count. */
export function validateApiListings(listed: string[], legacy: string[]) {
  const parallel = listed.filter((id) => parseCriterionId(id).mode === "par-4");
  if (
    !legacy.length ||
    !parallel.length ||
    new Set(listed).size !== listed.length ||
    new Set(legacy).size !== legacy.length ||
    listed.length !== legacy.length + parallel.length ||
    legacy.some((id) => parseCriterionId(id).mode !== "api" || !listed.includes(id))
  )
    throw new Error("Expected serial-only legacy IDs and additional four-thread parallel cases");
  return { serial: legacy[0], parallel };
}

/** Build and verify the same static artifacts uploaded by the unprivileged CI job. */
export async function buildContinuous(root: string, out: string) {
  if (process.platform !== "linux" || process.arch !== "x64")
    throw new Error("Continuous artifacts require Linux x86-64 with glibc");
  await mkdir(out, { recursive: false });
  await withWorkspace(root, async ({ tree, target }) => {
    for (const variant of VARIANTS) {
      const artifact = await buildBenchmark(tree, target, "smoke", variant.features, {
        targetTriple: "x86_64-unknown-linux-gnu",
        env: {
          RUSTFLAGS: undefined,
          CARGO_ENCODED_RUSTFLAGS: [
            "-C",
            "target-feature=+crt-static",
            "-C",
            "llvm-args=-x86-branches-within-32B-boundaries",
          ].join("\u001f"),
        },
      });
      const linkage = await command("ldd", [artifact.executable], { capture: true });
      if (!linkage.includes("statically linked"))
        throw new Error(`${variant.name} executable is not statically linked`);
      // Exercise the new exact-ID contract in this unprivileged build job.
      // --test verifies outputs without measurements or a four-CPU prerequisite.
      const listed = (
        await command(artifact.executable, ["--bench"], {
          capture: true,
          env: { TESSERA_BENCH_LIST: "1", TESSERA_BENCH_PARALLEL: "4" },
        })
      )
        .split("\n")
        .filter(Boolean);
      const legacy = (
        await command(artifact.executable, ["--bench"], {
          capture: true,
          env: { TESSERA_BENCH_LIST: "1", TESSERA_BENCH_PARALLEL: undefined },
        })
      )
        .split("\n")
        .filter(Boolean);
      const { serial, parallel } = validateApiListings(listed, legacy);
      await command(artifact.executable, ["--bench", "--test"], {
        env: { TESSERA_BENCH_PARALLEL: "4", TESSERA_BENCH_IDS: [serial, ...parallel].join("\n") },
      });
      const directory = join(out, variant.id);
      await mkdir(directory);
      await copyFile(artifact.executable, join(directory, "bench"));
      await writeFile(
        join(directory, "provenance.json"),
        `${JSON.stringify(artifact.provenance, null, 2)}\n`,
      );
    }
  });
}

/** Run the cloud scheduler locally without a container, service or credentials. */
export async function runContinuous(root: string, binary: string, out: string, plan?: Plan) {
  if (plan && plan.harness !== "smoke")
    throw new Error("Continuous scheduling requires a smoke harness plan");
  await mkdir(out, { recursive: false });
  const path = join(out, "plan.json");
  if (plan) await writeFile(path, `${JSON.stringify(plan)}\n`);
  const output = await command(process.execPath, ["bench/continuous/worker.ts", resolve(binary)], {
    cwd: root,
    capture: true,
    env: { TMPDIR: out, ...(plan ? { TESSERA_BENCH_PLAN: path } : {}) },
  });
  await writeFile(join(out, "criterion.txt"), `${output}\n`);
}

export interface MeasurementOptions {
  image: string;
  hash: string;
  project: string;
  out: string;
  pr?: string;
  startPoint?: string;
  baseHash?: string;
}

/** Select Bencher's baseline protocol without coupling it to GitHub Actions. */
export function baselineArguments(
  options: Pick<MeasurementOptions, "pr" | "startPoint" | "baseHash">,
) {
  if (Boolean(options.startPoint) !== Boolean(options.baseHash))
    throw new Error("--start-point and --base-hash must be supplied together");
  if (options.startPoint && options.baseHash)
    return [
      "--start-point",
      options.startPoint,
      "--start-point-hash",
      options.baseHash,
      "--start-point-clone-thresholds",
      "--start-point-reset",
    ];
  if (options.pr) return [];
  return [
    "--threshold-measure",
    "latency",
    "--threshold-test",
    "percentage",
    "--threshold-min-sample-size",
    "1",
    "--threshold-max-sample-size",
    "1",
    "--threshold-upper-boundary",
    "0.05",
    "--threshold-lower-boundary",
    "0.05",
    "--thresholds-reset",
  ];
}

/**
 * Measure both configurations, retaining each report and outcome even on failure.
 *
 * The image must be an immutable digest. Credentials come from BENCHER_API_KEY;
 * only this explicit remote command submits jobs. Its result records per-build
 * failures rather than skipping the second build or conflating them with alerts.
 */
export async function measureContinuous(options: MeasurementOptions) {
  const baseline = baselineArguments(options);
  if (!/^\S+@sha256:[a-f0-9]{64}$/.test(options.image))
    throw new Error("--image must be an immutable sha256 image reference");
  if (options.pr && !/^[1-9]\d*$/.test(options.pr))
    throw new Error("PR number must be a positive integer");
  const key = process.env.BENCHER_API_KEY;
  if (!key) throw new Error("BENCHER_API_KEY is required for remote measurement");
  await mkdir(options.out, { recursive: false });
  const outcomes: Record<string, "success" | "failure"> = {};
  for (const variant of VARIANTS) {
    try {
      const report = await command(
        "bencher",
        [
          "run",
          "--project",
          options.project,
          "--key",
          key,
          "--branch",
          options.pr ? `pr-${options.pr}` : "main",
          "--hash",
          options.hash,
          "--testbed",
          variant.testbed,
          "--spec",
          "intel-v1",
          "--image",
          options.image,
          "--job-timeout",
          "240",
          "--adapter",
          "rust_criterion",
          "--format",
          "json",
          "--quiet",
          ...baseline,
          "--exec",
          "--",
          "/usr/local/bin/node",
          "/bench/continuous/worker.ts",
          `/bench-${variant.id}`,
        ],
        { capture: true },
      );
      await writeFile(join(options.out, `bencher-${variant.id}.json`), `${report}\n`);
      outcomes[variant.id] = "success";
    } catch (error) {
      if (error instanceof CommandCancelledError) throw error;
      diagnostic(`${variant.name}: ${error instanceof Error ? error.message : String(error)}`);
      outcomes[variant.id] = "failure";
    }
    await writeFile(join(options.out, "outcomes.json"), `${JSON.stringify(outcomes, null, 2)}\n`);
  }
  return outcomes;
}
