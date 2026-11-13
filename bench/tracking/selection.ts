// SPDX-License-Identifier: BSD-2-Clause

import { z } from "zod";
import { parseCriterionId, revisionSampling } from "../catalog.ts";
import type { TrackingAlgorithm } from "./algorithms.ts";

export const inventorySchema = z
  .object({ supported: z.array(z.string()), unsupported: z.array(z.string()) })
  .superRefine(({ supported, unsupported }, ctx) => {
    const all = [...supported, ...unsupported];
    if (!supported.length || new Set(all).size !== all.length)
      ctx.addIssue({ code: "custom", message: "Empty or overlapping tracking inventory" });
    for (const id of all) {
      try {
        parseCriterionId(id);
      } catch {
        ctx.addIssue({ code: "custom", message: "Invalid tracking case: " + id });
      }
    }
  });
export type TrackingInventory = z.infer<typeof inventorySchema>;

/** Validate the frozen executable's selection against the smoke listing, including declined pairs. */
export function validateInventory(
  input: unknown,
  reference: string[],
  algorithm: TrackingAlgorithm,
) {
  const inventory = inventorySchema.parse(input);
  const expected = new Set(
    reference
      .filter((id) => parseCriterionId(id).mode === (algorithm.threads ? "par-4" : "api"))
      .map((id) => (algorithm.threads ? id.replace(/par-4$/, "par-" + algorithm.threads) : id)),
  );
  const all = [...inventory.supported, ...inventory.unsupported];
  if (expected.size !== all.length || all.some((id) => !expected.has(id)))
    throw new Error("Algorithm inventory differs from the shared main plan");
  return inventory;
}

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
