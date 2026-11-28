// SPDX-License-Identifier: BSD-2-Clause

import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { z } from "zod";
import { command } from "../../tools/process.ts";
import { repositoryRoot, runTask } from "../../tools/tasks.ts";
import { columnId } from "../catalog.ts";
import { localImage, type TrackingBuild } from "./build.ts";
import { reportFailure } from "./failure.ts";
import { columnSlug, trackingBranch, trackingColumn } from "./implementations.ts";
import { JOB_TIMEOUT_S } from "./jobs.ts";
import { combineReports, runSchema, summarizeReport, TRACKING_SPEC } from "./result.ts";

/** A detached submission's pending report names the remote job to attach to. */
const pendingSchema = z.object({ job: z.uuid() });

/**
 * Seconds to wait for all of a run's jobs, queueing included. The workflow's
 * `track` timeout must exceed it by the time to load and push the image.
 */
const QUEUE_WAIT_S = 50 * 60;

// CI-only: push the image `build.ts` froze, measure its remote jobs in order, then summarize
// the complete run. The workflow owns event policy, credentials, serialization and Git publication.
const root = await repositoryRoot();
const column = trackingColumn(process.env.TRACKING_COLUMN ?? "tessera/serial");
const input = join(root, "bench/results/tracking-build");
const out = join(root, "bench/results/tracking");
await reportFailure(`Track (${columnId(column)})`, `${out}.log`, () =>
  runTask(out, false, async () => {
    const build: TrackingBuild = JSON.parse(await readFile(join(input, "build.json"), "utf8"));
    const project = process.env.BENCHER_PROJECT ?? "";
    if (
      !/^[a-z0-9-]+$/.test(project) ||
      build.column !== columnId(column) ||
      process.env.GITHUB_SHA !== build.hash ||
      process.env.GITHUB_REF !== "refs/heads/main"
    )
      throw new Error("Expected Bencher project and this column's exact main Actions build");
    await mkdir(out);
    await command("docker", ["load", "-i", join(input, "image.tar")]);
    const imageTag = `registry.bencher.dev/${project}:${columnSlug(column)}-${process.env.GITHUB_RUN_ID}-${process.env.GITHUB_RUN_ATTEMPT}`;
    await command("docker", ["tag", localImage(column), imageTag]);
    await command("docker", ["push", imageTag]);
    const image = await command(
      "docker",
      ["image", "inspect", imageTag, "--format", "{{index .RepoDigests 0}}"],
      { capture: true },
    );
    const branch = trackingBranch(column.implementation);
    const adapter = build.go ? "json" : "rust_criterion";
    // Submit every job first so that they queue together, then wait for each in order.
    // The CLI's own wait is twice the job timeout and counts queueing, so attach instead.
    const submitted = [];
    for (const [index, ids] of build.jobs.entries()) {
      const pending = await command(
        "bencher",
        [
          "run",
          `--project=${project}`,
          `--branch=${branch}`,
          `--hash=${build.hash}`,
          "--testbed=main",
          `--spec=${TRACKING_SPEC.slug}`,
          `--image=${image}`,
          `--job-timeout=${JOB_TIMEOUT_S}`,
          `--adapter=${adapter}`,
          "--detach",
          "--format=json",
          "--quiet",
          "/app/run.sh",
          `/app/job-${index + 1}.ids`,
        ],
        { capture: true },
      );
      submitted.push({ index, ids, job: pendingSchema.parse(JSON.parse(pending)).job });
    }
    const deadline = Date.now() + QUEUE_WAIT_S * 1000;
    const jobs = [];
    for (const { index, ids, job } of submitted) {
      // The shared deadline bounds every attachment, so the workflow keeps time to publish.
      const wait = Math.floor((deadline - Date.now()) / 1000);
      if (wait < 1)
        throw new Error(
          `Timed out waiting for remote job after ${QUEUE_WAIT_S} seconds: the run's shared queue wait is spent before job ${index + 1} of ${submitted.length} finished`,
        );
      const response = await command(
        "bencher",
        [
          "run",
          `--project=${project}`,
          `--job=${job}`,
          `--job-timeout=${wait}`,
          "--format=json",
          "--quiet",
        ],
        { capture: true },
      );
      await writeFile(join(out, `bencher-${index + 1}.json`), response);
      jobs.push(summarizeReport(JSON.parse(response), build.hash, project, ids, branch, adapter));
    }
    const run = runSchema.parse({
      ...combineReports(jobs),
      execution: column.execution,
      commit_time: build.commit_time,
      image,
      workflow: `${process.env.GITHUB_SERVER_URL}/${process.env.GITHUB_REPOSITORY}/actions/runs/${process.env.GITHUB_RUN_ID}/attempts/${process.env.GITHUB_RUN_ATTEMPT}`,
      go: build.go,
      rustc: build.rustc,
      rustflags: build.rustflags,
      memory: build.memory,
      ...(build.native ? { native: build.native } : {}),
    });
    await writeFile(join(out, run.time.slice(0, 7) + ".jsonl"), JSON.stringify(run) + "\n");
    return { column: columnId(column), hash: build.hash, jobs: jobs.length, image };
  }),
);
