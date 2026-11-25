// SPDX-License-Identifier: BSD-2-Clause

import { copyFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { command } from "../../tools/process.ts";
import { repositoryRoot, runTask } from "../../tools/tasks.ts";
import {
  type Column,
  columnId,
  continuous,
  criterionId,
  isComposition,
  operationKey,
  parseCaseId,
  parseCriterionId,
  SORT,
} from "../catalog.ts";
import { loadPlan, type Plan, plansByOperation } from "../plan.ts";
import { withComparisonWorkspace } from "../release.ts";
import { memoryRows, verifiedPeak } from "../report/model.ts";
import {
  benchmarkEnvironment,
  buildBenchmark,
  resolveCommit,
  workingTreeDirty,
} from "../workspace.ts";
import { reportFailure } from "./failure.ts";
import { columnSlug, trackingColumn, trackingImplementation } from "./implementations.ts";
import { remoteJobs } from "./jobs.ts";
import { runSchema, type TrackingRun } from "./result.ts";

/** Local image name shared by the build and measurement stages; the registry tag is the measurer's. */
export const localImage = (column: Column) => `tessera-tracking:${columnSlug(column)}`;

/** What the build stage hands to measurement, next to the saved image. */
export type TrackingBuild = {
  /** The measured column ID. */
  column: string;
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
 * The continuous plan's inputs for one column: a serial column takes the serial rules, including
 * every operation's, and a parallel column the parallel rules at its own pool size. An
 * implementation lists only the cases its adapter supports. A composition column also sorts the
 * parallel rules' inputs serially, once for every pool size, as the parallel figure's baseline.
 */
export function columnPlan(plan: Plan, column: Column): Plan {
  const parallel = column.execution !== "serial";
  const composition = isComposition(column.implementation);
  const include = plan.include
    .filter((rule) => {
      const serialSortForParallel = composition && (rule.operation ?? SORT).kind === SORT.kind;
      return (rule.executions ?? ["serial"]).some(
        (e) => (e !== "serial") === (parallel || serialSortForParallel),
      );
    })
    .map((rule) => ({ ...rule, executions: [column.execution] }));
  if (!include.length) throw new Error(`The continuous plan has no rules for ${columnId(column)}`);
  return { ...plan, implementations: [column.implementation], include };
}

/**
 * Key the memory worker's rows by timed Criterion ID and require one row per timed case.
 *
 * The worker measures every column of the plan; rows for a column the run does not time,
 * such as Tessera's beside `glidesort-sqrt`, are dropped.
 */
export function trackingMemory(journal: string, column: Column, ids: string[]) {
  const timed = new Set(ids);
  const peaks: Record<string, number> = {};
  for (const row of memoryRows(journal)) {
    if (row.build !== "default" || row.column !== columnId(column)) continue;
    const id = criterionId(parseCaseId(row.case_id), column.execution);
    if (!timed.has(id)) continue;
    const peak = verifiedPeak(row);
    if (id in peaks) throw new Error("Duplicate memory row: " + id);
    peaks[id] = peak;
  }
  const missing = ids.filter((id) => !(id in peaks));
  if (missing.length) throw new Error("Missing memory rows: " + missing.join(", "));
  return peaks;
}

// CI-only, without credentials: freeze one implementation into one tested image.
// Builds of different implementations run in parallel; measurement (`run.ts`) is serialized.
if (import.meta.main) {
  const root = await repositoryRoot();
  const column = trackingColumn(process.env.TRACKING_COLUMN ?? "tessera/serial");
  const implementation = trackingImplementation(column.implementation);
  const out = join(root, "bench/results/tracking-build");
  await reportFailure(`Build (${columnId(column)})`, `${out}.log`, () =>
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
      const native = implementation.adapter === "native";
      return withComparisonWorkspace(root, native, async ({ tree, target }) => {
        const env = await benchmarkEnvironment(tree, {
          targetTriple: "x86_64-unknown-linux-gnu",
          env: { CC: "clang", CXX: "clang++" },
        });
        const features = [...continuous.features, ...(native ? ["compare-native"] : [])];
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
        const plan = columnPlan(await loadPlan("continuous"), column);
        await writeFile(join(context, "plan.json"), JSON.stringify(plan));
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
        const measured = new Set(ids.map((id) => operationKey(parseCriterionId(id).operation)));
        // Peak extra allocation per case, measured by the same instrumented worker as local
        // comparisons. Counts do not depend on the host, so Actions measures them once per build.
        // The image carries the rows: `glidesort-sqrt` sizes its buffer from Tessera's peak.
        const memory = await buildBenchmark(tree, target, "memory", features, {
          profile: "bench",
          targetTriple: "x86_64-unknown-linux-gnu",
          env,
        });
        // The worker measures one operation per session. The sort session's directory is the
        // image's scratch evidence; every session's rows give the peaks.
        const journals: string[] = [];
        for (const [index, part] of plansByOperation(plan).entries()) {
          const operation = part.include[0].operation ?? SORT;
          if (!measured.has(operationKey(operation))) continue;
          const sort = operation.kind === SORT.kind;
          const directory = sort ? join(context, "scratch") : join(out, `memory-${index}`);
          const partPlan = join(out, `memory-plan-${index}.json`);
          await writeFile(
            partPlan,
            JSON.stringify(
              sort && implementation.id === "glidesort-sqrt"
                ? { ...part, implementations: ["tessera", implementation.id] }
                : part,
            ),
          );
          await command(memory.executable, ["--plan", partPlan, "--out", directory]);
          journals.push(await readFile(join(directory, "memory.jsonl"), "utf8"));
        }
        const peaks = trackingMemory(journals.join("\n"), column, ids);
        const jobs = remoteJobs(ids);
        for (const [index, job] of jobs.entries())
          await writeFile(join(context, `job-${index + 1}.ids`), job.join("\n") + "\n");
        for (const name of ["Dockerfile", "run.sh"])
          await copyFile(join(root, "bench/tracking", name), join(context, name));
        const image = localImage(column);
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
          column: columnId(column),
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
        return {
          column: columnId(column),
          hash: source,
          cases: ids.length,
          jobs: jobs.length,
        };
      });
    }),
  );
}
