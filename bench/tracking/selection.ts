// SPDX-License-Identifier: BSD-2-Clause

import { parseCriterionId, revisionSampling } from "../catalog.ts";

/** Partition whole cases, preserving sampling budgets. Short nominal shards leave room for
 * calibration, minimum complete sorts and slow opponents; the remote 300s limit remains authoritative.
 * Nominal budgets cannot prove wall time: remote slowest-adapter pilots are required. */
export function trackingShards(ids: string[]) {
  const limit_ms = 60_000;
  const cost = (id: string) => {
    const settings = revisionSampling(parseCriterionId(id).len);
    return settings.warm_ms + settings.window_ms;
  };
  const count = Math.ceil(ids.reduce((sum, id) => sum + cost(id), 0) / limit_ms);
  const shards = Array.from({ length: Math.max(1, count) }, () => ({
    ids: [] as string[],
    budget_ms: 0,
  }));
  for (const id of ids.toSorted((a, b) => cost(b) - cost(a) || a.localeCompare(b))) {
    const shard = shards.reduce((a, b) => (a.budget_ms <= b.budget_ms ? a : b));
    shard.ids.push(id);
    shard.budget_ms += cost(id);
  }
  return shards.map((shard) => shard.ids);
}
