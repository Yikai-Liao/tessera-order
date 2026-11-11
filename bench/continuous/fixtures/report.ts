// SPDX-License-Identifier: BSD-2-Clause

import { VARIANTS } from "../../catalog.ts";
import type { BencherReport } from "../report.ts";

export const hash = "a".repeat(40);
export const baseHash = "b".repeat(40);

/** Cloud identity; tests supply the actual paired ratios independently. */
export function fixture(): BencherReport {
  return {
    uuid: "r-1",
    start_time: "2026-11-07T02:00:00Z",
    end_time: "2026-11-07T02:01:00Z",
    project: { slug: "proj" },
    testbed: { uuid: "t-1", name: VARIANTS[0].relativeTestbed, spec: { uuid: "spec-1" } },
    branch: {
      uuid: "br-pr",
      name: "pr-8",
      head: {
        uuid: "h-pr",
        version: { hash },
        start_point: { branch: "br-main", head: "h-main", version: { hash: baseHash } },
      },
    },
    results: [[]],
  };
}
