// SPDX-License-Identifier: BSD-2-Clause

import { createHash } from "node:crypto";
import { chmod, copyFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { command } from "../../tools/process.ts";
import { repositoryRoot, runTask } from "../../tools/tasks.ts";
import { parseCriterionId } from "../catalog.ts";
import { loadPlan } from "../plan.ts";
import {
  benchmarkEnvironment,
  buildBenchmark,
  resolveCommit,
  withWorkspace,
  workingTreeDirty,
} from "../workspace.ts";
import { summarizeReport, type TrackingRun } from "./result.ts";

const hash = (bytes: string | Uint8Array) => createHash("sha256").update(bytes).digest("hex");

/** CI-only complete operation: build one static GNU image, measure remotely, retain a validated summary.
 * Credentials and Git/site publication remain workflow responsibilities; no partial result is published.
 */
async function measureMain(root: string, out: string) {
  if (await workingTreeDirty(root)) throw new Error("Tracking requires a clean committed checkout");
  const source = await resolveCommit(root, "HEAD");
  const project = process.env.BENCHER_PROJECT ?? "";
  if (!/^[a-z0-9-]+$/.test(project) || process.env.GITHUB_SHA !== source)
    throw new Error("Expected Bencher project and exact Actions source");
  await mkdir(out);
  return withWorkspace(root, async ({ tree, target }) => {
    const env = await benchmarkEnvironment(tree, { targetTriple: "x86_64-unknown-linux-gnu" });
    env.CARGO_ENCODED_RUSTFLAGS += "\x1f-C\x1ftarget-feature=+crt-static";
    const built = await buildBenchmark(tree, target, "smoke", ["rayon"], {
      targetTriple: "x86_64-unknown-linux-gnu",
      env,
    });
    const headers = await command("readelf", ["-l", "-d", built.executable], { capture: true });
    if (/INTERP|\(NEEDED\)/.test(headers)) throw new Error("Benchmark is not statically linked");
    const context = join(out, "image");
    await mkdir(context);
    const executable = join(context, "benchmark");
    await copyFile(built.executable, executable);
    await chmod(executable, 0o755);
    await command("strip", ["--strip-debug", executable]);
    const plan = JSON.stringify(await loadPlan("main"));
    await writeFile(join(context, "plan.json"), plan);
    const ids = (
      await command(executable, [], {
        capture: true,
        env: {
          TESSERA_BENCH_PLAN: join(context, "plan.json"),
          TESSERA_BENCH_LIST: "1",
        },
      })
    ).split("\n");
    for (const mode of ["api", "par-4"])
      await writeFile(
        join(context, mode + ".ids"),
        ids.filter((id) => parseCriterionId(id).mode === mode).join("\n") + "\n",
      );
    for (const name of ["Dockerfile", "run.sh"])
      await copyFile(join(root, "bench/tracking", name), join(context, name));
    const imageTag =
      "registry.bencher.dev/" +
      project +
      ":main-" +
      process.env.GITHUB_RUN_ID +
      "-" +
      process.env.GITHUB_RUN_ATTEMPT;
    await command("docker", ["build", "-t", imageTag, context]);
    await command("docker", [
      "run",
      "--rm",
      "--network",
      "none",
      imageTag,
      "./benchmark",
      "--test",
    ]);
    const size = await command("docker", ["image", "inspect", imageTag, "--format", "{{.Size}}"], {
      capture: true,
    });
    await command("docker", ["push", imageTag]);
    const image = await command(
      "docker",
      ["image", "inspect", imageTag, "--format", "{{index .RepoDigests 0}}"],
      { capture: true },
    );
    const response = await command(
      "bencher",
      [
        "run",
        "--project",
        project,
        "--branch",
        "main",
        "--hash",
        source,
        "--testbed",
        "baremetal-intel-v1-absolute-v1",
        "--spec",
        "intel-v1",
        "--image",
        image,
        "--job-timeout",
        "300",
        "--adapter",
        "rust_criterion",
        "--format",
        "json",
        "--quiet",
        "/app/run.sh",
      ],
      { capture: true },
    );
    await writeFile(join(out, "bencher.json"), response);
    const run: TrackingRun = {
      ...summarizeReport(JSON.parse(response), source, project, ids),
      protocol: "criterion-main-v1",
      image,
      workflow:
        process.env.GITHUB_SERVER_URL +
        "/" +
        process.env.GITHUB_REPOSITORY +
        "/actions/runs/" +
        process.env.GITHUB_RUN_ID +
        "/attempts/" +
        process.env.GITHUB_RUN_ATTEMPT,
      rustc: built.provenance.rustc,
      rustflags: built.provenance.cargo_encoded_rustflags,
      plan_hash: hash(plan),
      executable_hash: hash(await readFile(executable)),
      lock_hash: hash(await readFile(join(tree, "Cargo.lock"))),
    };
    await writeFile(join(out, run.time.slice(0, 7) + ".jsonl"), JSON.stringify(run) + "\n");
    return { hash: source, cases: ids.length, image, image_bytes: Number(size) };
  });
}

if (import.meta.main) {
  const root = await repositoryRoot();
  const out = join(root, "bench/results/main");
  await runTask(out, false, () => measureMain(root, out));
}
