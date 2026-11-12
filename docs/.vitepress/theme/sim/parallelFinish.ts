// SPDX-License-Identifier: BSD-2-Clause
// Models block/parallel.rs directory adoption and parallel/finish.rs transfers.
// Bucket orders are serial linearizations of direct-streamed completion;
// mutex scheduling, key panics and raw-pointer provenance remain Rust tests.
import { type Rec, simulate } from "./blockPartition.ts";

interface Source {
  logical: number;
  len: number;
}

/** Replays source-ledger completion from the serial model's scattered shards. */
export function simulateParallelFinish(config: {
  keys: number[];
  buckets: number;
  blockSize: number;
  shards: number;
  classify: (key: number) => number;
  order?: number[];
}) {
  const { keys, buckets: R, blockSize: b, classify } = config;
  const fullBlocks = Math.floor(keys.length / b);
  const shards = Math.min(config.shards, Math.max(1, fullBlocks));
  const scratchBlocks = 2 * shards * R;
  const blockCount = scratchBlocks + fullBlocks;
  const cells: (Rec | null)[][] = Array.from({ length: blockCount }, () =>
    Array<Rec | null>(b).fill(null),
  );
  const tail = Array<Rec | null>(keys.length % b).fill(null);
  const FREE = -1;
  const FINAL = -2;
  const owners = Array<number>(blockCount).fill(FREE);
  const locations: number[] = [];
  const sources: Source[][] = Array.from({ length: R }, () => []);
  const frames: { action: string; live: number[] }[] = [];
  const completed = Array<boolean>(R).fill(false);
  let firstBlock = 0;
  const local = Array.from({ length: shards }, (_, shard) => {
    const blocks = Math.floor(fullBlocks / shards) + Number(shard < fullBlocks % shards);
    const begin = firstBlock * b;
    const end = shard + 1 === shards ? keys.length : begin + blocks * b;
    const result = simulate({ keys: keys.slice(begin, end), buckets: R, blockSize: b, classify });
    const frame = result.frames.find((frame) => frame.phase === "finish")!;
    const item = { begin, firstBlock, frame };
    firstBlock += blocks;
    return item;
  });
  // Bucket-major and original shard order, exactly as the Rust adoption loop.
  for (let bucket = 0; bucket < R; bucket++) {
    for (let shard = 0; shard < shards; shard++) {
      const { frame, begin, firstBlock } = local[shard];
      for (let logical = R; logical < frame.activeEnd; logical++) {
        if (frame.bucketIds[logical] !== bucket) continue;
        const physical = frame.order[logical];
        const scratch = physical < 2 * R;
        const records = (scratch ? frame.scratch : frame.input)
          .slice(
            (scratch ? physical : physical - 2 * R) * b,
            ((scratch ? physical : physical - 2 * R) + 1) * b,
          )
          .filter((cell) => cell.live)
          .map((cell) => ({ ...cell.rec!, id: cell.rec!.id + begin }));
        if (!records.length) continue;
        const global = scratch
          ? shard * 2 * R + physical
          : scratchBlocks + firstBlock + physical - 2 * R;
        const source = locations.length;
        cells[global].splice(0, records.length, ...records);
        owners[global] = source;
        locations.push(global);
        sources[bucket].push({ logical: source, len: records.length });
      }
    }
  }
  const free = Array.from({ length: R + 1 }, () => [] as number[]);
  owners.forEach((owner, physical) => {
    if (owner === FREE) free[R].push(physical);
  });
  const starts: number[] = [];
  let end = 0;
  for (const bucket of sources) {
    starts.push(end);
    end += bucket.reduce((len, source) => len + source.len, 0);
  }
  const snapshot = (action: string) => {
    frames.push({
      action,
      live: [...cells.flat(), ...tail]
        .filter((record) => record !== null)
        .map((record) => record.id),
    });
  };
  let relocations = 0;
  const recordTransfers = { final: 0, relocation: 0 };
  const takeFree = (bucket: number) => {
    for (const cache of [
      bucket,
      R,
      ...Array.from({ length: R - 1 }, (_, i) => (bucket + i + 1) % R),
    ]) {
      while (free[cache].length) {
        const physical = free[cache].pop()!;
        if (owners[physical] === FREE) return physical;
      }
    }
    throw new Error("No free relocation block");
  };
  const protect = (physical: number, bucket: number) => {
    const source = owners[physical];
    if (source === FINAL) return;
    if (source !== FREE) {
      const target = takeFree(bucket);
      recordTransfers.relocation += cells[physical].filter((record) => record !== null).length;
      cells[target] = cells[physical];
      cells[physical] = Array<Rec | null>(b).fill(null);
      owners[target] = source;
      locations[source] = target;
      relocations++;
    }
    owners[physical] = FINAL;
    snapshot("protect");
  };
  const writeRecord = (record: Rec, position: number) => {
    const block = Math.floor(position / b);
    if (block < fullBlocks) {
      cells[scratchBlocks + block][position % b] = record;
    } else {
      tail[position % b] = record;
    }
    recordTransfers.final++;
  };
  const protectBand = (start: number, len: number, bucket: number) => {
    for (let position = start; position < start + len; ) {
      const block = Math.floor(position / b);
      if (block < fullBlocks) protect(scratchBlocks + block, bucket);
      position += Math.min(b - (position % b), start + len - position);
    }
  };
  snapshot("adopt");
  for (const bucket of config.order ?? Array.from({ length: R }, (_, bucket) => bucket)) {
    if (completed[bucket]) throw new Error("Bucket already completed");
    let start = starts[bucket];
    for (const source of sources[bucket]) {
      protectBand(start, source.len, bucket);
      // Protection can relocate this source. Resolve it only afterwards.
      const physical = locations[source.logical];
      if (owners[physical] !== source.logical) throw new Error("Source owner changed");
      const records = cells[physical].slice(0, source.len) as Rec[];
      cells[physical] = Array<Rec | null>(b).fill(null);
      owners[physical] = FREE;
      free[bucket].push(physical);
      records.forEach((record, offset) => {
        writeRecord(record, start + offset);
      });
      start += source.len;
      snapshot(`stream prefix ${bucket}`);
    }
    completed[bucket] = true;
  }
  const output = [...cells.slice(scratchBlocks).flat(), ...tail].filter(
    (record) => record !== null,
  );
  return { output, frames, relocations, recordTransfers };
}
