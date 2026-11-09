// SPDX-License-Identifier: BSD-2-Clause
// Models block/parallel.rs directory adoption and parallel/finish.rs transfers.
// Actions are serial linearizations of whole-bucket staging and final writes;
// mutex scheduling, batch-cache release before callbacks, key panics and
// raw-pointer provenance remain Rust tests.
import { type Rec, simulate } from "./blockPartition.ts";

export interface FinishAction {
  phase: "stage" | "write";
  bucket: number;
}

interface Source {
  logical: number;
  len: number;
}

/** Replays bounded finishing from the serial model's scattered shard frames. */
export function simulateParallelFinish(config: {
  keys: number[];
  buckets: number;
  blockSize: number;
  shards: number;
  classify: (key: number) => number;
  actions?: FinishAction[];
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
  const staged: (Rec | null)[][] = Array.from({ length: R }, () => []);
  const ready = Array<boolean>(R).fill(false);
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
      live: [...cells.flat(), ...tail, ...staged.flat()]
        .filter((record) => record !== null)
        .map((record) => record.id),
    });
  };
  let relocations = 0;
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
      cells[target] = cells[physical];
      cells[physical] = Array<Rec | null>(b).fill(null);
      owners[target] = source;
      locations[source] = target;
      relocations++;
    }
    owners[physical] = FINAL;
    snapshot("protect");
  };
  snapshot("adopt");
  const actions =
    config.actions ??
    Array.from({ length: R }, (_, bucket) => [
      { phase: "stage", bucket } as const,
      { phase: "write", bucket } as const,
    ]).flat();
  for (const { phase, bucket } of actions) {
    if (phase === "stage") {
      if (ready[bucket]) throw new Error("Bucket staged twice");
      const records: Rec[] = [];
      for (const source of sources[bucket]) {
        const physical = locations[source.logical];
        if (owners[physical] !== source.logical) throw new Error("Source owner changed");
        records.push(...(cells[physical].slice(0, source.len) as Rec[]));
        cells[physical] = Array<Rec | null>(b).fill(null);
        owners[physical] = FREE;
        free[bucket].push(physical);
      }
      records.sort((a, b) => a.key - b.key);
      staged[bucket] = records;
      ready[bucket] = true;
    } else {
      if (!ready[bucket]) throw new Error("Bucket written before staging");
      staged[bucket].forEach((record, offset) => {
        const position = starts[bucket] + offset;
        const block = Math.floor(position / b);
        if (block < fullBlocks) {
          const physical = scratchBlocks + block;
          protect(physical, bucket);
          cells[physical][position % b] = record;
        } else {
          tail[position % b] = record;
        }
        staged[bucket][offset] = null;
      });
    }
    snapshot(`${phase} ${bucket}`);
  }
  const output = [...cells.slice(scratchBlocks).flat(), ...tail].filter(
    (record) => record !== null,
  );
  return { output, frames, relocations };
}
