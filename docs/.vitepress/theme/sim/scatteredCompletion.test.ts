// SPDX-License-Identifier: BSD-2-Clause

// Abstract cost/recovery model of the rejected tile-only ordered-stream B
// construction, not a Rust implementation, allocator or concurrency model.
import assert from "node:assert/strict";
import { test } from "node:test";

interface RecordId {
  id: number;
  key: number;
}

function complete(
  keys: number[],
  buckets: number,
  blockSize: number,
  streamed: boolean,
  fail?: number,
) {
  // Both alternatives receive this same stable root directory. Every root
  // bucket fits one leaf; no descendant partition or avoided packing exists.
  const sources = Array.from({ length: buckets }, (_, bucket) => {
    const records = keys
      .map((key, id) => ({ key, id }))
      .filter((record) => Math.floor(record.key / 16) === bucket);
    return Array.from({ length: Math.ceil(records.length / blockSize) }, (_, block) =>
      records.slice(block * blockSize, (block + 1) * blockSize),
    );
  });
  const output: RecordId[] = [];
  const stream: RecordId[][] = [];
  const lane: RecordId[] = [];
  const moves = { gather: 0, append: 0, final: 0 };
  const events: string[] = [];
  const frames: number[][] = [];
  let failed = false;
  const snapshot = () => {
    const ids = [...sources.flat(2), ...lane, ...stream.flat(), ...output].map(
      (record) => record.id,
    );
    frames.push(ids);
  };
  const append = () => {
    while (lane.length) {
      let tail = stream.at(-1);
      if (!tail || tail.length === blockSize) {
        tail = [];
        stream.push(tail);
      }
      tail.push(lane.shift()!);
      moves.append++;
      snapshot();
    }
  };
  snapshot();
  for (const bucket of sources) {
    for (const prefix of bucket) {
      while (prefix.length) {
        lane.push(prefix.shift()!);
        moves.gather++;
        snapshot();
      }
    }
    if (!failed) {
      try {
        lane.sort((a, b) => {
          if (a.id === fail || b.id === fail) throw new Error("key panic");
          return a.key - b.key;
        });
      } catch {
        failed = true;
        events.push("panic");
      }
    }
    // On error all later movement is callback-free. The tile operation owns
    // every ID during sorting, so restoring it means consuming that same set.
    if (streamed) append();
    else {
      while (lane.length) {
        output.push(lane.shift()!);
        moves.final++;
        snapshot();
      }
    }
  }
  events.push("join");
  if (streamed) {
    events.push("terminal");
    for (const prefix of stream) {
      while (prefix.length) {
        output.push(prefix.shift()!);
        moves.final++;
        snapshot();
      }
    }
  }
  return { output, moves, events, frames, failed };
}

test("ordered-stream B adds a transfer and a post-join chain in matched tile-only roots", () => {
  for (const blockSize of [1, 3, 7]) {
    const keys = Array.from({ length: 97 }, (_, id) => (id * 37 + 11) % 64);
    const expected = keys.map((key, id) => ({ key, id })).sort((a, b) => a.key - b.key);
    for (const streamed of [false, true]) {
      for (const fail of [undefined, 0, 31]) {
        const result = complete(keys, 4, blockSize, streamed, fail);
        assert.equal(result.failed, fail !== undefined);
        if (!result.failed) assert.deepEqual(result.output, expected);
        for (const ids of result.frames) {
          assert.equal(ids.length, keys.length);
          assert.equal(new Set(ids).size, keys.length);
        }
        assert.deepEqual(result.moves, {
          gather: keys.length,
          append: streamed ? keys.length : 0,
          final: keys.length,
        });
        if (streamed) assert.ok(result.events.indexOf("terminal") > result.events.indexOf("join"));
      }
    }
  }
});
