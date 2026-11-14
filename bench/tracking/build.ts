// SPDX-License-Identifier: BSD-2-Clause

import { copyFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { command } from "../../tools/process.ts";
import { repositoryRoot, runTask } from "../../tools/tasks.ts";
import { algorithm as catalogAlgorithm } from "../catalog.ts";
import { loadPlan } from "../plan.ts";
import { withComparisonWorkspace } from "../release.ts";
import {
  benchmarkEnvironment,
  buildBenchmark,
  resolveCommit,
  workingTreeDirty,
} from "../workspace.ts";
import { trackingAlgorithm } from "./algorithms.ts";
import { reportFailure } from "./failure.ts";
import { remoteJobs } from "./jobs.ts";
import { runSchema, type TrackingRun } from "./result.ts";

/** Local image name shared by the build and measurement stages; the registry tag is the measurer's. */
export const localImage = (algorithm: string) => `tessera-tracking:${algorithm}`;

/** What the build stage hands to measurement, next to the saved image. */
export type TrackingBuild = {
  algorithm: string;
  hash: string;
  commit_time: string;
  rustc: string;
  rustflags: string;
  native?: TrackingRun["native"];
  /** Peak extra allocation bytes per timed case ID. */
  memory: Record<string, number>;
  /** Case IDs of each remote job, in run order; `job-N.ids` in the image holds job N. */
  jobs: string[][];
};

/**
 * Key the memory worker's rows by timed case ID and require one row per timed case.
 *
 * The worker measures each selected algorithm on every input of the plan; rows for an
 * algorithm or mode the run does not time are dropped. A row's mode follows its algorithm.
 */
export function trackingMemory(journal: string, algorithms: string[], ids: string[]) {
  const timed = new Set(ids);
  const peaks: Record<string, number> = {};
  for (const line of journal.split("\n").filter(Boolean)) {
    const row = JSON.parse(line) as {
      case_id: string;
      variant: string;
      build: string;
      peak_extra_bytes: number;
      verified: boolean;
    };
    if (row.build !== "default" || !algorithms.includes(row.variant)) continue;
    const threads = catalogAlgorithm(row.variant).threads;
    const id = `rust/${row.case_id}/${threads ? `par-${threads}` : "api"}`;
    if (!timed.has(id)) continue;
    if (!row.verified || !Number.isSafeInteger(row.peak_extra_bytes) || row.peak_extra_bytes < 0)
      throw new Error("Invalid memory row: " + id);
    if (id in peaks) throw new Error("Duplicate memory row: " + id);
    peaks[id] = row.peak_extra_bytes;
  }
  const missing = ids.filter((id) => !(id in peaks));
  if (missing.length) throw new Error("Missing memory rows: " + missing.join(", "));
  return peaks;
}

// CI-only, without credentials: freeze one algorithm into one tested image.
// Builds of different algorithms run in parallel; measurement (`run.ts`) is serialized.
if (import.meta.main) {
  const root = await repositoryRoot();
  const algorithm = trackingAlgorithm(process.env.TRACKING_ALGORITHM ?? "tessera");
  const out = join(root, "bench/results/tracking-build");
  await reportFailure(`Build (${algorithm.id})`, `${out}.log`, () =>
    runTask(out, false, async () => {
      if (await workingTreeDirty(root))
        throw new Error("Tracking requires a clean committed checkout");
      const source = await resolveCommit(root, "HEAD");
      if (process.env.GITHUB_SHA !== source || process.env.GITHUB_REF !== "refs/heads/main")
        throw new Error("Expected the exact main Actions source");
      const commitTime = new Date(
        await command("git", ["show", "-s", "--format=%cI", source], { cwd: root, capture: true }),
      ).toISOString();
      await mkdir(out);
      const native = algorithm.adapter === "native";
      return withComparisonWorkspace(root, native, async ({ tree, target }) => {
        const env = await benchmarkEnvironment(tree, {
          targetTriple: "x86_64-unknown-linux-gnu",
          env: { CC: "clang", CXX: "clang++" },
        });
        const features = ["rayon", ...(native ? ["compare-native"] : [])];
        const built = await buildBenchmark(tree, target, "smoke", features, {
          targetTriple: "x86_64-unknown-linux-gnu",
          env,
        });
        const headers = await command("readelf", ["-l", "-d", built.executable], { capture: true });
        if (!native && /INTERP|\(NEEDED\)/.test(headers))
          throw new Error("Benchmark is not statically linked");
        if (native) {
          const libraries = await command("ldd", [built.executable], { capture: true });
          if (/libtbb|libgomp|libomp/.test(libraries))
            throw new Error("Unexpected parallel native runtime");
        }
        const context = join(out, "image");
        await mkdir(context);
        await copyFile(built.executable, join(context, "benchmark"));
        await command("strip", ["--strip-debug", join(context, "benchmark")]);
        // Tessera's history keeps both continuous modes; an opponent replaces the algorithm selection.
        const plan = await loadPlan(
          "continuous",
          algorithm.id === "tessera" ? {} : { algorithms: [algorithm.id] },
        );
        await writeFile(join(context, "plan.json"), JSON.stringify(plan));
        // Peak extra allocation per case, measured by the same instrumented worker as local
        // comparisons. Counts do not depend on the host, so Actions measures them once per build.
        // The image carries the rows: `glidesort-sqrt` sizes its buffer from Tessera's peak.
        const memory = await buildBenchmark(tree, target, "memory", features, {
          profile: "bench",
          targetTriple: "x86_64-unknown-linux-gnu",
          env,
        });
        const memoryAlgorithms =
          algorithm.id === "glidesort-sqrt" ? ["tessera", algorithm.id] : plan.algorithms;
        await writeFile(
          join(out, "memory-plan.json"),
          JSON.stringify({ ...plan, algorithms: memoryAlgorithms }),
        );
        await command(memory.executable, [
          "--plan",
          join(out, "memory-plan.json"),
          "--out",
          join(context, "scratch"),
        ]);
        const ids = (
          await command(built.executable, [], {
            capture: true,
            env: { TESSERA_BENCH_PLAN: join(context, "plan.json"), TESSERA_BENCH_LIST: "1" },
          })
        )
          .split("\n")
          .filter(Boolean);
        if (!ids.length) throw new Error("Tracking plan selects no supported cases");
        await writeFile(join(context, "cases.ids"), ids.join("\n") + "\n");
        const peaks = trackingMemory(
          await readFile(join(context, "scratch/memory.jsonl"), "utf8"),
          plan.algorithms,
          ids,
        );
        const jobs = remoteJobs(ids);
        for (const [index, job] of jobs.entries())
          await writeFile(join(context, `job-${index + 1}.ids`), job.join("\n") + "\n");
        for (const name of ["Dockerfile", "run.sh"])
          await copyFile(join(root, "bench/tracking", name), join(context, name));
        const image = localImage(algorithm.id);
        await command("docker", [
          "build",
          "--target",
          native ? "native-runtime" : "static-runtime",
          "-t",
          image,
          context,
        ]);
        // Exercise every selected case and the real entrypoint without collecting timing evidence.
        await command("docker", ["run", "--rm", "--network=none", image, "/app/run.sh", "--test"]);
        await command("docker", ["save", "-o", join(out, "image.tar"), image]);
        const build: TrackingBuild = {
          algorithm: algorithm.id,
          hash: source,
          commit_time: commitTime,
          rustc: built.provenance.rustc,
          rustflags: built.provenance.cargo_encoded_rustflags,
          // Validated here so that a build without complete provenance fails before measurement.
          ...(native
            ? {
                native: runSchema.shape.native.parse({
                  cc: built.provenance.cc,
                  cxx: built.provenance.cxx,
                  build: built.provenance.native_build,
                }),
              }
            : {}),
          memory: peaks,
          jobs,
        };
        await writeFile(join(out, "build.json"), JSON.stringify(build));
        return { algorithm: algorithm.id, hash: source, cases: ids.length, jobs: jobs.length };
      });
    }),
  );
}
