// SPDX-License-Identifier: BSD-2-Clause

import { copyFile, mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { command } from "../../tools/process.ts";
import { repositoryRoot, runTask } from "../../tools/tasks.ts";
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
  /** Case IDs of each remote job, in run order; `job-N.ids` in the image holds job N. */
  jobs: string[][];
};

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
        if (!native) env.CARGO_ENCODED_RUSTFLAGS += "\x1f-C\x1ftarget-feature=+crt-static";
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
        if (algorithm.id === "glidesort-sqrt") {
          // Allocation counts do not depend on the host, so the image carries Tessera's
          // exact-case scratch instead of a second worker.
          const memory = await buildBenchmark(tree, target, "memory", features, {
            profile: "bench",
            targetTriple: "x86_64-unknown-linux-gnu",
            env,
          });
          await writeFile(
            join(out, "memory-plan.json"),
            JSON.stringify({ ...plan, algorithms: ["tessera"] }),
          );
          await command(memory.executable, [
            "--plan",
            join(out, "memory-plan.json"),
            "--out",
            join(context, "scratch"),
          ]);
        }
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
          jobs,
        };
        await writeFile(join(out, "build.json"), JSON.stringify(build));
        return { algorithm: algorithm.id, hash: source, cases: ids.length, jobs: jobs.length };
      });
    }),
  );
}
