// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { type Config, type Frame, simulate } from "./blockPartition.ts";
import { instantiate } from "./engine.ts";

// `bun run docs:test` builds this file first.
const engine = await instantiate(readFileSync(new URL("tessera_trace.wasm", import.meta.url)));

function assertOneLiveOwner(frames: Frame[], len: number, context: string) {
  for (const frame of frames) {
    const live = [...frame.scratch, ...frame.input]
      .filter((cell) => cell.live)
      .map((cell) => cell.rec!.id);
    assert.equal(live.length, len, `${context}: ${frame.caption}`);
    assert.equal(new Set(live).size, len, `${context}: ${frame.caption}`);
  }
}

function stableOrder({ keys, radix, divisor }: Config) {
  const bucket = (key: number) => Math.floor(key / divisor) & ((1 << radix) - 1);
  return keys
    .map((key, id) => ({ key, id }))
    .sort((a, b) => bucket(a.key) - bucket(b.key))
    .map((record) => record.id);
}

test("the replay ends in the engine's stable order with one live owner per frame", () => {
  // Empty buckets, a single bucket, full and partial blocks; lengths straddle
  // the first block and a later block boundary.
  for (const [radix, blockSize] of [
    [1, 1],
    [2, 3],
    [3, 6],
  ]) {
    const buckets = 1 << radix;
    for (const len of new Set([2, blockSize + 1, 8 * blockSize, 8 * blockSize + 1])) {
      for (const occupied of [1, buckets]) {
        const keys = Array.from({ length: len }, (_, id) => (id * 37 + 11) % (occupied * 10));
        const config = { keys, radix, blockSize, divisor: 10 };
        const { output, frames, transfers } = simulate(engine, config);
        const context = `radix=${radix} block=${blockSize} len=${len} occupied=${occupied}`;
        assert.deepEqual(
          output.map((record) => record.id),
          stableOrder(config),
          context,
        );
        assert.equal(transfers.final, len, context);
        assertOneLiveOwner(frames, len, context);
      }
    }
  }
});

test("completion relocates sources that lie in another bucket's band", () => {
  let relocated = 0;
  for (const [radix, blockSize, len] of [
    [1, 1, 33],
    [2, 3, 73],
    [3, 7, 127],
  ]) {
    for (const concentrated of [false, true]) {
      const keys = Array.from({ length: len }, (_, id) =>
        concentrated && id % 17 !== 0 ? 7 : (id * 7919 + 73) % 256,
      );
      const config = { keys, radix, blockSize, divisor: 1 };
      const result = simulate(engine, config);
      const context = `radix=${radix} block=${blockSize} concentrated=${concentrated}`;
      assert.deepEqual(
        result.output.map((record) => record.id),
        stableOrder(config),
        context,
      );
      assertOneLiveOwner(result.frames, len, context);
      // Each input block is protected once, moving at most its live prefix.
      assert.ok(result.transfers.relocation <= Math.floor(len / blockSize) * blockSize, context);
      relocated += result.transfers.relocation;
    }
  }
  assert.ok(relocated > 0);
});
