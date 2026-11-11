// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { type FinishAction, simulateParallelFinish } from "./parallelFinish.ts";

test("equality ranks preserve the shared sharded directory order", () => {
  // The rank closure uses the serial below/equal/above policy. This model checks
  // its shared scatter/adoption semantics; materialized completion and Rayon
  // scheduling are covered by the native Rust tests, not this bounded model.
  const equal = 7;
  const keys = Array.from({ length: 129 }, (_, id) => (id % 16 === 0 ? id % 17 : equal));
  const expected = keys.map((key, id) => ({ key, id })).sort((a, b) => a.key - b.key);
  for (const blockSize of [1, 3, 7]) {
    for (const shards of [2, 4]) {
      const result = simulateParallelFinish({
        keys,
        buckets: 4,
        blockSize,
        shards,
        classify: (key) => Number(key >= equal) + Number(key > equal),
      });
      assert.deepEqual(
        result.output.map((record) => record.id),
        expected.map((record) => record.id),
      );
      for (const frame of result.frames) {
        assert.equal(frame.live.length, keys.length);
        assert.equal(new Set(frame.live).size, keys.length);
      }
    }
  }
});

test("bounded finishing preserves records under immediate and suspended writes", () => {
  let relocations = 0;
  // Vary storage geometry independently of scheduling. Repeated keys in the
  // longest input cross shard boundaries; tiny blocks force relocation.
  for (const [buckets, blockSize, shards, len, keySpace] of [
    [2, 1, 2, 33, 256],
    [4, 3, 4, 73, 256],
    [8, 7, 2, 127, 4],
  ]) {
    const keys = Array.from({ length: len }, (_, id) => (id * 7919 + 73) % (buckets * keySpace));
    for (const [reverse, suspended] of [
      [false, false],
      [true, false],
      [false, true],
      [true, true],
    ]) {
      const order = Array.from({ length: buckets }, (_, i) => (reverse ? buckets - 1 - i : i));
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
  assert.ok(relocations > 0);
});
