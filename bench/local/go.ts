// SPDX-License-Identifier: BSD-2-Clause

import { cp, mkdir, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { command } from "../../tools/process.ts";
import { parseCriterionId, revisionSampling } from "../catalog.ts";
import type { Plan } from "../plan.ts";
import { buildGoWorker } from "../release.ts";
import { fileHash } from "../session.ts";
import {
  buildBenchmark,
  resolveCommit,
  sourceHash,
  type Workspace,
  withWorkspace,
} from "../workspace.ts";
import type { PreparedPair } from "./model.ts";

export interface GoBuild {
  GoVersion: string;
  Settings: { Key: string; Value: string }[];
  Deps: unknown[];
}

export interface GoRequest {
  id: string;
  case: { len: number };
  record_bytes: number;
  batch: number;
  implementation: string;
  threads: number;
  parallel: boolean;
  samples: number;
  warm_ns: number;
  window_ns: number;
}
export type GoRequests = Record<string, GoRequest>;

export async function goRequest(binary: string, request: object) {
  let result: Record<string, unknown> = {};
  await command(binary, [], {
    interact: async (exchange) => {
      result = JSON.parse(await exchange(JSON.stringify(request)));
      if (result.error) throw new Error(String(result.error));
    },
  });
  return result;
}

export async function goBuildInfo(binary: string) {
  const info = (await goRequest(binary, { action: "info" })) as unknown as GoBuild;
  if (
    !info.GoVersion ||
    !info.Settings?.some((setting) => setting.Key === "GOEXPERIMENT" && setting.Value === "simd")
  )
    throw new Error("Expected a Go SIMD benchmark build");
  return info;
}

/** The existing plan resolver owns numeric boundaries and batch limits. */
export async function goRequests(
  workspace: Workspace,
  plan: Plan,
  path: string,
  implementation: string,
) {
  await writeFile(path, JSON.stringify(plan));
  const { executable } = await buildBenchmark(workspace.tree, workspace.target, "compare", [], {
    profile: "dev",
  });
  const result = await command(executable, ["describe", path], { capture: true });
  const requests: GoRequests = {};
  for (const line of result.split("\n").filter(Boolean)) {
    const descriptor = JSON.parse(line);
    const { execution, len } = parseCriterionId(descriptor.id);
    const budget = revisionSampling(len);
    const parallel = execution !== "serial";
    requests[descriptor.id] = {
      ...descriptor,
      implementation,
      parallel,
      threads: parallel ? Number(execution.slice(1)) : 1,
      samples: budget.samples,
      warm_ns: Math.floor((budget.warm_ms * 1000) / budget.samples) * 1000,
      window_ns: Math.floor((budget.window_ms * 1000) / budget.samples) * 1000,
    };
  }
  return requests;
}

export async function listGo(binary: string, requests: GoRequests) {
  const supported: string[] = [];
  await command(binary, [], {
    interact: async (exchange) => {
      for (const request of Object.values(requests)) {
        const result = JSON.parse(await exchange(JSON.stringify({ ...request, action: "list" })));
        if (result.id !== request.id || Boolean(result.route) === Boolean(result.decline))
          throw new Error("Invalid Go worker inventory");
        if (result.route) supported.push(request.id);
      }
    },
  });
  return supported.sort();
}

export async function prepareGo(root: string, baseline: string, out: string, plan: Plan) {
  const head = await resolveCommit(root, "HEAD");
  baseline = await resolveCommit(root, baseline);
  const present = await command("git", ["ls-tree", baseline, "--", "go/tessera/go.mod"], {
    cwd: root,
    capture: true,
  });
  if (!present) return { kind: "no-baseline", baseline, head };
  if (
    await command("git", ["status", "--porcelain", "--untracked-files=no"], {
      cwd: root,
      capture: true,
    })
  )
    throw new Error("Go preparation requires a clean committed checkout");

  return withWorkspace(root, async (workspace) => {
    const old = join(workspace.directory, "baseline-source");
    const current = join(workspace.directory, "current-source");
    for (const [snapshot, revision, paths] of [
      [old, baseline, ["go/tessera"]],
      [current, head, ["go/tessera", "bench/go"]],
    ] as const) {
      await mkdir(snapshot);
      const archive = `${snapshot}.tar`;
      await command("git", ["archive", "-o", archive, revision, ...paths], { cwd: root });
      await command("tar", ["-xf", archive, "-C", snapshot]);
    }
    await mkdir(out);
    const requests = await goRequests(workspace, plan, join(out, "plan.json"), "go-tessera");
    const binaryPaths = { baseline: "baseline/worker", current: "current/worker" };
    const binaries = { baseline: "", current: "" };
    const builds = {} as NonNullable<PreparedPair["go"]>["builds"];
    const digest = {} as Record<"baseline" | "current", string[]>;
    const allCases = Object.keys(requests).sort();
    if (!allCases.length) throw new Error("Go plan selects no cases");
    for (const [role, source] of [
      ["baseline", old],
      ["current", current],
    ] as const) {
      const binary = await buildGoWorker(current, workspace, source);
      const retained = join(out, binaryPaths[role]);
      await mkdir(dirname(retained));
      await cp(binary, retained);
      binaries[role] = await fileHash(retained);
      builds[role] = await goBuildInfo(retained);
      if (JSON.stringify(await listGo(retained, requests)) !== JSON.stringify(allCases))
        throw new Error("Go Tessera inventory does not cover the prepared plan");
      // Compare each immutable input in the two worker builds without sampling.
      digest[role] = [];
      await command(retained, [], {
        interact: async (exchange) => {
          for (const id of allCases) {
            const reply = JSON.parse(
              await exchange(JSON.stringify({ ...requests[id], action: "digest" })),
            );
            if (reply.id !== id || !/^[a-f0-9]{16}$/.test(reply.digest))
              throw new Error("Missing Go input digest");
            digest[role].push(reply.digest);
          }
        },
      });
    }
    if (JSON.stringify(digest.baseline) !== JSON.stringify(digest.current))
      throw new Error("Go revision input digests differ");
    if (JSON.stringify(builds.baseline) !== JSON.stringify(builds.current))
      throw new Error("Go revision build settings differ");
    const pair: PreparedPair = {
      kind: "prepared-revisions",
      schema: 1,
      protocol: "paired-samples-v2",
      plan,
      baseline,
      head,
      dirty: false,
      source: {
        baseline: await sourceHash(join(old, "go/tessera")),
        current: await sourceHash(join(current, "go/tessera")),
        harness: await sourceHash(join(current, "bench/go")),
        catalog: await fileHash(join(root, "bench/catalog.toml")),
      },
      binaries,
      binaryPaths,
      configuration: "portable-simd",
      rustc: "",
      flags: "",
      cases: allCases,
      go: { builds, requests },
    };
    await writeFile(join(out, "prepared.json"), JSON.stringify(pair));
    return { kind: pair.kind, head, baseline, cases: allCases.length };
  });
}
