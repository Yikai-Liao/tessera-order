// SPDX-License-Identifier: BSD-2-Clause

import type { BencherReport, Boundary } from "../report.ts";

export const hash = "a".repeat(40);
export const baseHash = "b".repeat(40);
const latency = { uuid: "m-latency", name: "Latency" };
const model = {
  test: "t_test",
  min_sample_size: 5,
  max_sample_size: 32,
  lower_boundary: 0.999,
  upper_boundary: 0.999,
};

function result(name: string, value: number, boundary: Boundary | null) {
  return {
    benchmark: { uuid: `b-${name}`, name },
    measures: [
      {
        measure: latency,
        metric: { uuid: `metric-${name}`, value },
        boundary,
        threshold: boundary ? { model } : undefined,
      },
    ],
  };
}

export function fixture(): BencherReport {
  return {
    uuid: "r-1",
    start_time: "2026-11-07T02:00:00Z",
    end_time: "2026-11-07T02:01:00Z",
    project: { slug: "proj" },
    testbed: { uuid: "t-1", name: "intel-v1", spec: { uuid: "spec-1" } },
    branch: {
      uuid: "br-pr",
      name: "pr-8",
      head: {
        uuid: "h-pr",
        version: { hash },
        start_point: { branch: "br-main", head: "h-main", version: { hash: baseHash } },
      },
    },
    results: [
      [
        result("rust/u64/random/49/s1/api", 736, {
          baseline: 709,
          lower_limit: 705,
          upper_limit: 713,
        }),
        result("rust/u64/sorted99/1000000/s1/api", 733100, {
          baseline: 1033988,
          lower_limit: 986115,
          upper_limit: 1081862,
        }),
        result("rust/u64/equal/65536/s1/api", 26000, {
          baseline: 26031,
          lower_limit: 23707,
          upper_limit: 28354,
        }),
        result("rust/u32/sorted99/65536/s1/api", 41000, null),
        result("rust/row_u32/random/48/s1/api", 800, {
          baseline: 726,
          lower_limit: 725,
          upper_limit: 727,
        }),
      ],
    ],
    alerts: [
      { metric: { uuid: "metric-rust/u64/random/49/s1/api" }, limit: "upper", status: "active" },
      {
        metric: { uuid: "metric-rust/u64/sorted99/1000000/s1/api" },
        limit: "lower",
        status: "active",
      },
      {
        metric: { uuid: "metric-rust/row_u32/random/48/s1/api" },
        limit: "upper",
        status: "active",
      },
    ],
  };
}
