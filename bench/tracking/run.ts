// SPDX-License-Identifier: BSD-2-Clause

import { copyFile, mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { command } from "../../tools/process.ts";
import { repositoryRoot, runTask } from "../../tools/tasks.ts";
import { inventorySchema, parseCriterionId } from "../catalog.ts";
import { loadPlan } from "../plan.ts";
import { withComparisonWorkspace } from "../release.ts";
import {
  benchmarkEnvironment,
  buildBenchmark,
  resolveCommit,
  workingTreeDirty,
} from "../workspace.ts";
import { trackingAlgorithm, trackingBranch } from "./algorithms.ts";
import { mergeReports, runSchema, summarizeReport } from "./result.ts";
import { trackingShards } from "./selection.ts";

// CI-only: freeze one algorithm/image, measure complete-case shards, then publish one complete summary.
// The workflow owns event policy, credentials and Git publication.
const root = await repositoryRoot();
const algorithm = trackingAlgorithm(process.env.TRACKING_ALGORITHM ?? "tessera");
const out = join(root, "bench/results/tracking");
await runTask(out, false, async () => {
  if (await workingTreeDirty(root)) throw new Error("Tracking requires a clean committed checkout");
  const source = await resolveCommit(root, "HEAD");
  const commitTime = new Date(
    await command("git", ["show", "-s", "--format=%cI", source], { cwd: root, capture: true }),
  ).toISOString();
  const project = process.env.BENCHER_PROJECT ?? "";
  if (
    !/^[a-z0-9-]+$/.test(project) ||
    process.env.GITHUB_SHA !== source ||
    process.env.GITHUB_REF !== "refs/heads/main"
  )
    throw new Error("Expected Bencher project and exact main Actions source");
  await mkdir(out);
  const native = algorithm.adapter === "native";
  return withComparisonWorkspace(root, native, async ({ tree, target }) => {
    const env = await benchmarkEnvironment(tree, {
      targetTriple: "x86_64-unknown-linux-gnu",
      env: { CC: "clang", CXX: "clang++" },
    });
    if (!native) env.CARGO_ENCODED_RUSTFLAGS += "\x1f-C\x1ftarget-feature=+crt-static";
    const features = ["compare", "rayon", ...(native ? ["compare-native"] : [])];
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
    if (algorithm.id === "glidesort-sqrt") {
      const memory = await buildBenchmark(tree, target, "memory", features, {
        profile: "bench",
        targetTriple: "x86_64-unknown-linux-gnu",
        env,
      });
      await copyFile(memory.executable, join(context, "memory"));
    }
    await writeFile(join(context, "algorithm"), algorithm.id);
    await writeFile(
      join(context, "plan.json"),
      JSON.stringify(
        await loadPlan(
          "continuous",
          algorithm.id === "tessera" ? {} : { algorithms: [algorithm.id] },
        ),
      ),
    );
    const inventory = inventorySchema.parse(
      JSON.parse(
        await command(built.executable, [], {
          capture: true,
          env: {
            TESSERA_BENCH_PLAN: join(context, "plan.json"),
            TESSERA_BENCH_LIST: "json",
          },
        }),
      ),
    );
    const shards = trackingShards(inventory.supported);
    await writeFile(join(out, "inventory.json"), JSON.stringify(inventory));
    for (const [index, ids] of shards.entries()) {
      await writeFile(join(context, index + ".ids"), ids.join("\n") + "\n");
      if (algorithm.id === "glidesort-sqrt")
        await writeFile(
          join(context, `${index}.memory.json`),
          JSON.stringify({
            schema: 1,
            algorithms: ["tessera"],
            include: ids.map((id) => {
              const { ty, distribution, len } = parseCriterionId(id);
              return { types: [ty], distributions: [distribution], lengths: [len] };
            }),
          }),
        );
    }
    for (const name of ["Dockerfile", "run.sh"])
      await copyFile(join(root, "bench/tracking", name), join(context, name));
    const imageTag = `registry.bencher.dev/${project}:${algorithm.path}-${process.env.GITHUB_RUN_ID}-${process.env.GITHUB_RUN_ATTEMPT}`;
    await command("docker", [
      "build",
      "--target",
      native ? "native-runtime" : "static-runtime",
      "-t",
      imageTag,
      context,
    ]);
    // Exercise every selected case and the real entrypoint without collecting timing evidence.
    for (const index of shards.keys())
      await command("docker", [
        "run",
        "--rm",
        "--network=none",
        imageTag,
        "/app/run.sh",
        String(index),
        "--test",
      ]);
    await command("docker", ["push", imageTag]);
    const image = await command(
      "docker",
      ["image", "inspect", imageTag, "--format", "{{index .RepoDigests 0}}"],
      { capture: true },
    );
    const reports = [];
    for (const [index, ids] of shards.entries()) {
      const response = await command(
        "bencher",
        [
          "run",
          `--project=${project}`,
          `--branch=${trackingBranch(algorithm.path)}`,
          `--hash=${source}`,
          "--testbed=main",
          "--spec=intel-v1",
          `--image=${image}`,
          "--job-timeout=300",
          "--adapter=rust_criterion",
          "--format=json",
          "--quiet",
          `/app/run.sh ${index}`,
        ],
        { capture: true },
      );
      await writeFile(join(out, "bencher-" + index + ".json"), response);
      reports.push(
        summarizeReport(JSON.parse(response), source, project, ids, trackingBranch(algorithm.path)),
      );
    }
    const run = runSchema.parse({
      ...mergeReports(reports, inventory.supported),
      algorithm: algorithm.path,
      unsupported: inventory.unsupported,
      commit_time: commitTime,
      protocol: "criterion-kernel-v2",
      image,
      workflow: `${process.env.GITHUB_SERVER_URL}/${process.env.GITHUB_REPOSITORY}/actions/runs/${process.env.GITHUB_RUN_ID}/attempts/${process.env.GITHUB_RUN_ATTEMPT}`,
      rustc: built.provenance.rustc,
      rustflags: built.provenance.cargo_encoded_rustflags,
      ...(native
        ? {
            native: {
              cc: built.provenance.cc,
              cxx: built.provenance.cxx,
              build: built.provenance.native_build,
            },
          }
        : {}),
    });
    await writeFile(join(out, run.time.slice(0, 7) + ".jsonl"), JSON.stringify(run) + "\n");
    return {
      algorithm: algorithm.path,
      hash: source,
      cases: inventory.supported.length,
      unsupported: inventory.unsupported.length,
      shards: shards.length,
      image,
    };
  });
});
