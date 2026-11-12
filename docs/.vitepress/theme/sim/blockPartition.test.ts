// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { type Frame, simulate } from "./blockPartition.ts";

function assertOneLiveOwner(frames: Frame[], len: number, context: string) {
  for (const frame of frames) {
    const live = [...frame.scratch, ...frame.input]
      .filter((cell) => cell.live)
      .map((cell) => cell.rec!.id);
    assert.equal(live.length, len, `${context}: ${frame.caption}`);
    assert.equal(new Set(live).size, len, `${context}: ${frame.caption}`);
  }
}

function stableOrder(keys: number[], classify: (key: number) => number) {
  return keys
    .map((key, id) => ({ key, id }))
    .sort((a, b) => classify(a.key) - classify(b.key))
    .map((record) => record.id);
}

test("serial partition preserves stable bucket order and one live owner per frame", () => {
  // Cover empty buckets, a single bucket, and recycling with full and partial
  // blocks. Lengths straddle the first block and a later block boundary.
  for (const [buckets, blockSize] of [
    [2, 1],
    [4, 3],
    [8, 6],
  ]) {
    for (const len of new Set([2, blockSize, blockSize + 1, 8 * blockSize, 8 * blockSize + 1])) {
      for (const occupied of [1, buckets]) {
        const keys = Array.from({ length: len }, (_, id) => (id * 37 + 11) % (occupied * 10));
        const classify = (key: number) => Math.floor(key / 10);
        const { output, frames, transfers } = simulate({ keys, buckets, blockSize, classify });
        const context = `buckets=${buckets} block=${blockSize} len=${len} occupied=${occupied}`;
        assert.deepEqual(
          output.map((record) => record.id),
          stableOrder(keys, classify),
          context,
        );
        assert.equal(transfers.final, len, context);
        assertOneLiveOwner(frames, len, context);
      }
    }
  }
});

test("equality digits preserve the shared sharded ledger order", () => {
  // The rank classifier: below/equal/above plus the top varying bit, so all
  // four buckets are occupied.
  const equal = 7;
  const top = 16;
  const keys = Array.from({ length: 129 }, (_, id) => (id % 16 === 0 ? id % 17 : equal));
  const rank = (key: number) =>
    Number(key >= equal) + Number(key > equal) + Number((key & top) !== 0);
  for (const blockSize of [1, 3, 7]) {
    for (const shards of [2, 4]) {
      const context = `block=${blockSize} shards=${shards}`;
      const result = simulate({ keys, buckets: 4, blockSize, shards, classify: rank });
      assert.deepEqual(
        result.output.map((record) => record.id),
        stableOrder(keys, rank),
        context,
      );
      assertOneLiveOwner(result.frames, keys.length, context);
    }
  }
});

test("any bucket order streams sources directly into their final bands", () => {
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
      for (const reverse of [false, true]) {
        const order = Array.from({ length: buckets }, (_, i) => (reverse ? buckets - 1 - i : i));
        const context = `buckets=${buckets} block=${blockSize} shards=${shards} reverse=${reverse}`;
        const result = simulate({ keys, buckets, blockSize, shards, classify, order });
        assert.deepEqual(
          result.output.map((record) => record.id),
          stableOrder(keys, classify),
          context,
        );
        assertOneLiveOwner(result.frames, len, context);
        assert.equal(result.transfers.final, len, context);
        // Each input block is protected once, moving at most its live prefix.
        assert.ok(result.transfers.relocation <= Math.floor(len / blockSize) * blockSize, context);
        relocatedRecords += result.transfers.relocation;
      }
    }
  }
  assert.ok(relocatedRecords > 0);
});
