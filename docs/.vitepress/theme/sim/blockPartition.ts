// SPDX-License-Identifier: BSD-2-Clause
// A step-by-step model of one stable block partition pass and its compaction,
// following `rust/tessera-sort/src/block.rs`: `Sorter::new`, `begin_pass`,
// `scatter`, `finish_pass` and `compact_with`. It moves record identities
// instead of bytes and records a frame after every observable step, so the
// explanation pages can replay the algorithm. Index arithmetic matches the
// Rust source line for line; `blockPartition.test.ts` checks that the result
// is the stable order for every small configuration.

export interface Rec {
  /** Position in the original input; equal buckets must keep this order. */
  id: number;
  key: number;
  bucket: number;
}

export interface Cell {
  rec: Rec | null;
  /** False for stale bytes: a record that has been moved elsewhere. */
  live: boolean;
}

export type Region = "scratch" | "input";

export interface Loc {
  region: Region;
  index: number;
}

export interface Writer {
  physical: number;
  next: number;
}

export type Phase = "setup" | "scatter" | "finish" | "compact" | "done";

export interface Frame {
  phase: Phase;
  caption: string;
  scratch: Cell[];
  input: Cell[];
  /** Logical position -> physical block. */
  order: number[];
  activeEnd: number;
  writers: Writer[] | null;
  /** Bucket that filled each logical output position during scatter. */
  bucketIds: (number | null)[];
  read: Loc | null;
  wrote: Loc[];
  /** Physical blocks to emphasise in this frame. */
  focus: number[];
}

export interface Config {
  keys: number[];
  buckets: number;
  blockSize: number;
  classify: (key: number) => number;
}

export interface Result {
  frames: Frame[];
  output: Rec[];
  blockCount: number;
  scratchBlocks: number;
  fullBlocks: number;
}

interface PartialBlock {
  orderIndex: number;
  len: number;
}

/** Short physical block names used in captions: S0.. for scratch, I0.. for input. */
export function blockName(physical: number, scratchBlocks: number): string {
  return physical < scratchBlocks ? `S${physical}` : `I${physical - scratchBlocks}`;
}

export function simulate(config: Config): Result {
  const { keys, blockSize: b, classify } = config;
  const R = config.buckets;
  const n = keys.length;
  const scratchBlocks = 2 * R;
  const fullBlocks = Math.floor(n / b);
  const tailLen = n % b;
  const blockCount = fullBlocks + scratchBlocks;
  const name = (p: number) => blockName(p, scratchBlocks);

  const scratch: Cell[] = Array.from({ length: scratchBlocks * b }, () => ({
    rec: null,
    live: false,
  }));
  const input: Cell[] = keys.map((key, id) => ({
    rec: { id, key, bucket: classify(key) },
    live: true,
  }));

  const blockLoc = (physical: number, offset: number): Loc =>
    physical < scratchBlocks
      ? { region: "scratch", index: physical * b + offset }
      : { region: "input", index: (physical - scratchBlocks) * b + offset };
  const cells = (region: Region) => (region === "scratch" ? scratch : input);
  const at = (loc: Loc) => cells(loc.region)[loc.index];

  // memmove semantics: read every source cell before writing any destination.
  const move = (from: Loc, to: Loc, len: number) => {
    const moved: (Rec | null)[] = [];
    for (let i = 0; i < len; i++) {
      const cell = cells(from.region)[from.index + i];
      moved.push(cell.live ? cell.rec : null);
      cell.live = false;
    }
    for (let i = 0; i < len; i++) {
      const cell = cells(to.region)[to.index + i];
      const rec = moved[i];
      cell.rec = rec ?? cell.rec;
      cell.live = rec !== null;
    }
  };

  const frames: Frame[] = [];
  let order: number[] = [];
  let activeEnd = 0;
  let writers: Writer[] | null = null;
  let bucketIds: (number | null)[] = [];
  const snap = (phase: Phase, caption: string, extra: Partial<Frame> = {}) => {
    frames.push({
      phase,
      caption,
      scratch: scratch.map((c) => ({ ...c })),
      input: input.map((c) => ({ ...c })),
      order: [...order],
      activeEnd,
      writers: writers ? writers.map((w) => ({ ...w })) : null,
      bucketIds: [...bucketIds],
      read: null,
      wrote: [],
      focus: [],
      ...extra,
    });
  };

  snap(
    "setup",
    `${n} records, ${R} buckets, blocks of ${b}. The input holds ${fullBlocks} full blocks ` +
      `(I0–I${fullBlocks - 1}) and a tail of ${tailLen}; scratch holds 2R = ${scratchBlocks} blocks.`,
  );

  // BlockStorage::new: the tail has no full physical block, so it starts in
  // scratch block R; scratch blocks 0..R stay free for the first outputs.
  move({ region: "input", index: fullBlocks * b }, blockLoc(R, 0), tailLen);
  activeEnd = R + fullBlocks + 1;
  order = Array.from({ length: blockCount }, (_, i) =>
    i < R ? i : i < activeEnd - 1 ? i + R : i - (activeEnd - 1) + R,
  );
  const partial: PartialBlock[] = Array.from({ length: R }, () => ({
    orderIndex: blockCount,
    len: 0,
  }));
  partial[0] = { orderIndex: activeEnd - 1, len: tailLen };
  bucketIds = Array.from({ length: blockCount }, () => null);
  snap(
    "setup",
    `The ${tailLen}-record tail moves to ${name(R)}. The logical block order now reads ` +
      `[free × ${R}] [I0 … I${fullBlocks - 1}, ${name(R)}] [free × ${R - 1}].`,
    { focus: [R] },
  );

  const nextBlockLen = (logical: number, cursor: { i: number }) => {
    if (cursor.i < partial.length && partial[cursor.i].orderIndex === logical) {
      return partial[cursor.i++].len;
    }
    return b;
  };

  // begin_pass: the first R logical blocks become one output block per bucket.
  writers = [];
  const counts: number[] = [];
  for (let i = 0; i < R; i++) {
    writers.push({ physical: order[i], next: 0 });
    bucketIds[i] = i;
    counts.push(1);
  }
  snap(
    "scatter",
    `Each bucket receives one free block to fill: ` +
      writers.map((w, i) => `bucket ${i} → ${name(w.physical)}`).join(", ") +
      ".",
    { focus: writers.map((w) => w.physical) },
  );

  // scatter
  let output = R;
  const partialCursor = { i: partial.findIndex((p) => p.orderIndex >= R) };
  if (partialCursor.i < 0) partialCursor.i = partial.length;
  for (let logical = R; logical < activeEnd; logical++) {
    const source = order[logical];
    const length = nextBlockLen(logical, partialCursor);
    for (let offset = 0; offset < length; offset++) {
      const from = blockLoc(source, offset);
      const rec = at(from).rec!;
      const bucket = classify(rec.key);
      const writer = writers[bucket];
      const to = blockLoc(writer.physical, writer.next);
      move(from, to, 1);
      writer.next++;
      snap(
        "scatter",
        `Read key ${rec.key} from ${name(source)}; digit ${bucket} sends it to ${name(writer.physical)}.`,
        { read: from, wrote: [to], focus: [source, writer.physical] },
      );
      if (writer.next === b) {
        const claimed = order[output];
        const full = writer.physical;
        writers[bucket] = { physical: claimed, next: 0 };
        bucketIds[output] = bucket;
        counts[bucket]++;
        output++;
        snap(
          "scatter",
          `${name(full)} is full. Bucket ${bucket} takes the next block in logical order, ` +
            `${name(claimed)}, which has been read completely.`,
          { focus: [full, claimed] },
        );
      }
    }
  }

  // finish_pass: lay the filled blocks out bucket by bucket, in fill order.
  activeEnd = output;
  const starts: number[] = [];
  let start = R;
  for (const count of counts) {
    starts.push(start);
    start += count;
  }
  const next = new Array<number>(blockCount).fill(0);
  const nextIds = new Array<number | null>(blockCount).fill(null);
  for (let i = 0; i < activeEnd; i++) {
    const bucket = bucketIds[i]!;
    const j = starts[bucket]++;
    next[j] = order[i];
    nextIds[j] = bucket;
    if (writers[bucket].physical === order[i]) {
      partial[bucket] = { orderIndex: j, len: writers[bucket].next };
    }
  }
  for (let i = 0; i < R; i++) next[i] = order[activeEnd + i];
  for (let i = activeEnd + R; i < blockCount; i++) next[i] = order[i];
  activeEnd += R;
  order = next;
  bucketIds = nextIds;
  writers = null;
  snap(
    "finish",
    `Every record has moved, and no record moved twice. Only the table changes now: the logical order ` +
      `lists each bucket's blocks in the order they were filled, so reading it front to back gives the ` +
      `stable result. The last block of every bucket is partial.`,
  );

  // compact_with: materialise the logical order into the input slice.
  const positions = new Array<number>(blockCount).fill(0);
  for (let i = 0; i < blockCount; i++) positions[order[i]] = i;
  let written = 0;
  const cursor = { i: 0 };
  let freeLogical = 0;
  let freePhysical = order[freeLogical];
  const inputLoc = (index: number): Loc => ({ region: "input", index });
  for (let destination = scratchBlocks; destination < blockCount; destination++) {
    const logical = destination - R;
    let out = positions[destination];
    const source = order[logical];
    if (out > logical && out < activeEnd) {
      move(blockLoc(destination, 0), blockLoc(freePhysical, 0), b);
      snap(
        "compact",
        `${name(destination)} still holds a later block of the result. It is moved out to the ` +
          `free block ${name(freePhysical)} before it is overwritten.`,
        { focus: [destination, freePhysical] },
      );
      order[freeLogical] = destination;
      order[out] = freePhysical;
      positions[freePhysical] = out;
      positions[destination] = freeLogical;
      freeLogical = out;
      out = positions[destination];
    }
    const length = nextBlockLen(logical, cursor);
    const wrote = Array.from({ length }, (_, i) => inputLoc(written + i));
    move(blockLoc(source, 0), inputLoc(written), length);
    snap(
      "compact",
      `Copy logical block ${logical} (${name(source)}, ${length} record${length === 1 ? "" : "s"}) ` +
        `to input positions ${written}–${written + length - 1}.`,
      { focus: [source], wrote },
    );
    order[logical] = destination;
    order[out] = source;
    positions[source] = out;
    positions[destination] = logical;
    written += length;
    if (logical !== out) {
      freePhysical = source;
      freeLogical = out;
    }
  }
  for (let logical = blockCount - R; logical < activeEnd; logical++) {
    const source = order[logical];
    const length = nextBlockLen(logical, cursor);
    const wrote = Array.from({ length }, (_, i) => inputLoc(written + i));
    move(blockLoc(source, 0), inputLoc(written), length);
    snap(
      "compact",
      `Copy logical block ${logical} (${name(source)}, ${length} record${length === 1 ? "" : "s"}) ` +
        `to input positions ${written}–${written + length - 1}.`,
      { focus: [source], wrote },
    );
    written += length;
  }
  if (written !== n) throw new Error(`compaction wrote ${written} of ${n} records`);
  snap(
    "done",
    "The input slice holds every record once, grouped by digit, each group in input order.",
  );

  return {
    frames,
    output: input.map((c) => {
      if (!c.live || !c.rec) throw new Error("compaction left a hole");
      return c.rec;
    }),
    blockCount,
    scratchBlocks,
    fullBlocks,
  };
}
