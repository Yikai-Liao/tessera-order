// SPDX-License-Identifier: BSD-2-Clause

import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { command } from "../../tools/process.ts";
import { repositoryRoot, runTask } from "../../tools/tasks.ts";
import { trackingAlgorithm, trackingBranch } from "./algorithms.ts";
import { localImage, type TrackingBuild } from "./build.ts";
import { JOB_TIMEOUT_S } from "./jobs.ts";
import { combineReports, runSchema, summarizeReport } from "./result.ts";

// CI-only: push the image `build.ts` froze, measure its remote jobs in order, then summarize
// the complete run. The workflow owns event policy, credentials, serialization and Git publication.
const root = await repositoryRoot();
const algorithm = trackingAlgorithm(process.env.TRACKING_ALGORITHM ?? "tessera");
const input = join(root, "bench/results/tracking-build");
const out = join(root, "bench/results/tracking");
await runTask(out, false, async () => {
  const build: TrackingBuild = JSON.parse(await readFile(join(input, "build.json"), "utf8"));
  const project = process.env.BENCHER_PROJECT ?? "";
  if (
    !/^[a-z0-9-]+$/.test(project) ||
    build.algorithm !== algorithm.id ||
    process.env.GITHUB_SHA !== build.hash ||
    process.env.GITHUB_REF !== "refs/heads/main"
  )
    throw new Error("Expected Bencher project and this algorithm's exact main Actions build");
  await mkdir(out);
  await command("docker", ["load", "-i", join(input, "image.tar")]);
  const imageTag = `registry.bencher.dev/${project}:${algorithm.id}-${process.env.GITHUB_RUN_ID}-${process.env.GITHUB_RUN_ATTEMPT}`;
  await command("docker", ["tag", localImage(algorithm.id), imageTag]);
  await command("docker", ["push", imageTag]);
  const image = await command(
    "docker",
    ["image", "inspect", imageTag, "--format", "{{index .RepoDigests 0}}"],
    { capture: true },
  );
  const branch = trackingBranch(algorithm.id);
  const jobs = [];
  for (const [index, ids] of build.jobs.entries()) {
    const response = await command(
      "bencher",
      [
        "run",
        `--project=${project}`,
        `--branch=${branch}`,
        `--hash=${build.hash}`,
        "--testbed=main",
        "--spec=intel-v1",
        `--image=${image}`,
        `--job-timeout=${JOB_TIMEOUT_S}`,
        "--adapter=rust_criterion",
        "--format=json",
        "--quiet",
        "/app/run.sh",
        `/app/job-${index + 1}.ids`,
      ],
      { capture: true },
    );
    await writeFile(join(out, `bencher-${index + 1}.json`), response);
    jobs.push(summarizeReport(JSON.parse(response), build.hash, project, ids, branch));
  }
  const run = runSchema.parse({
    ...combineReports(jobs),
    commit_time: build.commit_time,
    image,
    workflow: `${process.env.GITHUB_SERVER_URL}/${process.env.GITHUB_REPOSITORY}/actions/runs/${process.env.GITHUB_RUN_ID}/attempts/${process.env.GITHUB_RUN_ATTEMPT}`,
    rustc: build.rustc,
    rustflags: build.rustflags,
    ...(build.native ? { native: build.native } : {}),
  });
  await writeFile(join(out, run.time.slice(0, 7) + ".jsonl"), JSON.stringify(run) + "\n");
  return { algorithm: algorithm.id, hash: build.hash, jobs: jobs.length, image };
});
