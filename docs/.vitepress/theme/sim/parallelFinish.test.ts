// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { simulateParallelFinish } from "./parallelFinish.ts";

test("equality ranks preserve the shared sharded directory order", () => {
  // The rank closure uses the serial below/equal/above policy. This model checks
  // its shared scatter/adoption semantics; materialized completion and Rayon
  // scheduling are covered by the native Rust tests, not this bounded model.
  const equal = 7;
  const keys = Array.from({ length: 129 }, (_, id) => (id % 16 === 0 ? id % 17 : equal));
  const rank = (key: number) => Number(key >= equal) + Number(key > equal);
  const expected = keys.map((key, id) => ({ key, id })).sort((a, b) => rank(a.key) - rank(b.key));
  for (const blockSize of [1, 3, 7]) {
    for (const shards of [2, 4]) {
      const result = simulateParallelFinish({
        keys,
        buckets: 4,
        blockSize,
        shards,
        classify: rank,
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

test("direct streaming preserves source order without a temporary record pass", () => {
  let relocatedRecords = 0;
  for (const [buckets, blockSize, shards, len] of [
    [2, 1, 2, 33],
    [4, 3, 2, 73],
    [8, 7, 4, 127],
  ]) {
    for (const concentrated of [false, true]) {
      const keys = Array.from({ length: len }, (_, id) =>
        concentrated && id % 17 !== 0 ? 7 : (id * 7919 + 73) % 256,
      );
      const classify = (key: number) => key % buckets;
      const expected = keys
        .map((key, id) => ({ key, id, bucket: classify(key) }))
        .sort((a, b) => a.bucket - b.bucket);
      for (const reverse of [false, true]) {
        const order = Array.from({ length: buckets }, (_, i) => (reverse ? buckets - 1 - i : i));
        const result = simulateParallelFinish({
          keys,
          buckets,
          blockSize,
          shards,
          classify,
          order,
        });
        assert.deepEqual(result.output, expected);
        for (const frame of result.frames) {
          assert.equal(frame.live.length, len, frame.action);
          assert.equal(new Set(frame.live).size, len, frame.action);
        }
        assert.equal(result.recordTransfers.final, len);
        // Each input block is protected once, copying at most its live prefix.
        assert.ok(result.recordTransfers.relocation <= Math.floor(len / blockSize) * blockSize);
        relocatedRecords += result.recordTransfers.relocation;
      }
    }
  }
  assert.ok(relocatedRecords > 0);
});
