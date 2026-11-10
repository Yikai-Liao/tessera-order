// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { simulate } from "./blockPartition.ts";

test("block partition preserves stable bucket order and one live owner per frame", () => {
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
        const { output, frames } = simulate({ keys, buckets, blockSize, classify });
        const expected = keys
          .map((key, id) => ({ key, id }))
          .sort((a, b) => classify(a.key) - classify(b.key));
        const context = `buckets=${buckets} block=${blockSize} len=${len} occupied=${occupied}`;
        assert.deepEqual(
          output.map((record) => record.id),
          expected.map((record) => record.id),
          context,
        );
        for (const frame of frames) {
          const live = [...frame.scratch, ...frame.input]
            .filter((cell) => cell.live)
            .map((cell) => cell.rec!.id);
          assert.equal(live.length, len, `${context}: ${frame.caption}`);
          assert.equal(new Set(live).size, len, `${context}: ${frame.caption}`);
        }
      }
    }
  }
});
