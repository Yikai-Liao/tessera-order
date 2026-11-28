// SPDX-License-Identifier: BSD-2-Clause

import { command } from "../../tools/process.ts";
import { executionThreads, parseCriterionId, revisionSampling } from "../catalog.ts";
import type { GoBuild } from "../tracking/result.ts";
import { buildBenchmark, type Workspace } from "../workspace.ts";

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
export async function goRequests(workspace: Workspace, path: string, implementation: string) {
  const { executable } = await buildBenchmark(workspace.tree, workspace.target, "compare", [], {
    profile: "dev",
    env: Object.fromEntries(
      ["CARGO_BUILD_TARGET", "CARGO_PROFILE_BENCH_DEBUG", "RUSTFLAGS", "CC", "CXX"]
        .filter((name) => process.env[name] === "")
        .map((name) => [name, undefined]),
    ),
  });
  const result = await command(executable, ["describe", path], { capture: true });
  const requests: GoRequests = {};
  for (const line of result.split("\n").filter(Boolean)) {
    const descriptor = JSON.parse(line);
    const { execution, len } = parseCriterionId(descriptor.id);
    const budget = revisionSampling(len);
    const threads = executionThreads(execution);
    requests[descriptor.id] = {
      ...descriptor,
      implementation,
      parallel: threads !== undefined,
      threads: threads ?? 1,
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

export async function goDigests(binary: string, requests: GoRequests, ids: string[]) {
  const digests: string[] = [];
  await command(binary, [], {
    interact: async (exchange) => {
      for (const id of ids) {
        const reply = JSON.parse(
          await exchange(JSON.stringify({ ...requests[id], action: "digest" })),
        );
        if (reply.id !== id || !/^[a-f0-9]{16}$/.test(reply.digest))
          throw new Error("Missing Go input digest");
        digests.push(reply.digest);
      }
    },
  });
  return digests;
}
