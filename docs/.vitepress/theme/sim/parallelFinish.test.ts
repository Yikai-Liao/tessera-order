// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { type FinishAction, simulateParallelFinish } from "./parallelFinish.ts";

test("bounded finishing preserves every record under suspended bucket schedules", () => {
  let relocations = 0;
  for (const buckets of [2, 4, 8]) {
    for (const blockSize of [1, 3, 7]) {
      for (const shards of [2, 4]) {
        // Repeated keys in the longest input cross shard boundaries.
        for (const [len, keySpace] of [
          [33, 256],
          [73, 256],
          [127, 4],
        ] as const) {
          const keys = Array.from(
            { length: len },
            (_, id) => (id * 7919 + 73) % (buckets * keySpace),
          );
          const order = Array.from({ length: buckets }, (_, i) => i);
          for (const reverse of [false, true]) {
            if (reverse) order.reverse();
            for (const suspended of [false, true]) {
              const stages: FinishAction[] = order.map((bucket) => ({ phase: "stage", bucket }));
              const writes: FinishAction[] = order.map((bucket) => ({ phase: "write", bucket }));
              const actions = suspended
                ? [...stages, ...writes]
                : stages.flatMap((stage, i) => [stage, writes[i]]);
              const result = simulateParallelFinish({
                keys,
                buckets,
                blockSize,
                shards,
                classify: (key) => Math.floor(key / keySpace),
                actions,
              });
              const expected = keys.map((key, id) => ({ key, id })).sort((a, b) => a.key - b.key);
              assert.deepEqual(
                result.output.map((record) => record.id),
                expected.map((record) => record.id),
              );
              for (const frame of result.frames) {
                assert.equal(frame.live.length, len, frame.action);
                assert.equal(new Set(frame.live).size, len, frame.action);
              }
              relocations += result.relocations;
            }
          }
        }
      }
    }
  }
  assert.ok(relocations > 0);
});
