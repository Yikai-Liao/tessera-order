// SPDX-License-Identifier: BSD-2-Clause
// A step-by-step model of one stable block partition: the scatter in
// `rust/tessera-sort/src/block.rs` (`BlockStorage::new`, `begin_pass`,
// `scatter`) and the source-ledger completion in `block/finish.rs`
// (`build_completion`, `Bucket::finish`, `transfer`, `protect`,
// `protect_band`, `take_free`). Several shards follow `block/parallel.rs`:
// each scatters its own block range with its own scratch, and one ledger
// completes them. It moves record identities instead of bytes and records a
// frame after every observable step. Buckets complete one at a time in
// `order`, a serial linearization of parallel completion; mutex scheduling,
// key panics and pointer provenance remain Rust tests.

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

/** One initialized prefix of a physical block in a bucket's chain. */
export interface Prefix {
  physical: number;
  len: number;
}

/** Ledger state of a physical block: the bucket whose source it holds, or free or final. */
export type Owner = number | "free" | "final";

export type Phase = "setup" | "scatter" | "complete" | "done";

export interface Frame {
  phase: Phase;
  caption: string;
  scratch: Cell[];
  input: Cell[];
  /** Each bucket's current output block during scatter. */
  writers: Writer[] | null;
  /** Each bucket's unretired source prefixes, in stable order. */
  chains: Prefix[][];
  /** Per physical block, once the ledger exists. */
  owners: Owner[] | null;
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
  /** Requested scatter shards; clamped like `partition_impl_with_visit`. */
  shards?: number;
  /** Bucket completion order; ascending by default. */
  order?: number[];
}

export interface Result {
  frames: Frame[];
  output: Rec[];
  blockCount: number;
  scratchBlocks: number;
  fullBlocks: number;
  /** Records written to their final band, and records moved out of a destination first. */
  transfers: { final: number; relocation: number };
}

/** Short physical block names used in captions: S0.. for scratch, I0.. for input. */
export function blockName(physical: number, scratchBlocks: number): string {
  return physical < scratchBlocks ? `S${physical}` : `I${physical - scratchBlocks}`;
}

export function simulate(config: Config): Result {
  const { keys, blockSize: b, classify } = config;
  const R = config.buckets;
  const n = keys.length;
  const fullBlocks = Math.floor(n / b);
  const shards = Math.min(config.shards ?? 1, n, Math.max(1, fullBlocks));
  const localScratch = 2 * R;
  const scratchBlocks = shards * localScratch;
  const blockCount = scratchBlocks + fullBlocks;
  const name = (p: number) => blockName(p, scratchBlocks);
  const blocks = (list: Prefix[]) => list.map((prefix) => name(prefix.physical)).join(", ");

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
  const inputLoc = (index: number): Loc => ({ region: "input", index });
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
      if (rec === null) throw new Error("moved a stale record");
      cell.rec = rec;
      cell.live = true;
    }
  };

  const frames: Frame[] = [];
  const chains: Prefix[][] = Array.from({ length: R }, () => []);
  let writers: Writer[] | null = null;
  let owners: Owner[] | null = null;
  const snap = (phase: Phase, caption: string, extra: Partial<Frame> = {}) => {
    frames.push({
      phase,
      caption,
      scratch: scratch.map((c) => ({ ...c })),
      input: input.map((c) => ({ ...c })),
      writers: writers ? writers.map((w) => ({ ...w })) : null,
      chains: chains.map((chain) => chain.map((prefix) => ({ ...prefix }))),
      owners: owners ? [...owners] : null,
      read: null,
      wrote: [],
      focus: [],
      ...extra,
    });
  };

  snap(
    "setup",
    `${n} records, ${R} buckets, blocks of ${b}. The input holds ${fullBlocks} full blocks ` +
      `and a tail of ${n % b}; scratch holds 2R = ${localScratch} blocks` +
      (shards > 1 ? ` per shard, for ${shards} shards.` : "."),
  );

  // Each shard scatters its own consecutive full blocks; the last one also
  // reads the input tail. Shard chains are kept apart until the ledger joins
  // them in bucket-major, shard order.
  const shardChains: Prefix[][][] = [];
  let firstBlock = 0;
  for (let shard = 0; shard < shards; shard++) {
    const ownBlocks = Math.floor(fullBlocks / shards) + Number(shard < fullBlocks % shards);
    const begin = firstBlock * b;
    const len = shard + 1 === shards ? n - begin : ownBlocks * b;
    const scratchBase = shard * localScratch;
    // begin_pass: bucket k starts its chain in this shard's scratch block k.
    writers = Array.from({ length: R }, (_, k) => ({ physical: scratchBase + k, next: 0 }));
    const local: Prefix[][] = Array.from({ length: R }, (_, k) => [
      { physical: scratchBase + k, len: 0 },
    ]);
    for (let k = 0; k < R; k++) chains[k] = local[k];
    snap(
      "scatter",
      `Each bucket starts its chain in a scratch block: ` +
        writers.map((w, k) => `bucket ${k} → ${name(w.physical)}`).join(", ") +
        `. Blocks ${name(scratchBase + R)}–${name(scratchBase + 2 * R - 1)} stay free.`,
      { focus: writers.map((w) => w.physical) },
    );
    let output = 0;
    for (let offset = 0; offset < len; offset++) {
      const from = inputLoc(begin + offset);
      const rec = at(from).rec!;
      const bucket = classify(rec.key);
      const writer = writers[bucket];
      const to = blockLoc(writer.physical, writer.next);
      move(from, to, 1);
      writer.next++;
      const chain = local[bucket];
      chain[chain.length - 1].len = writer.next;
      snap(
        "scatter",
        `Read key ${rec.key}; digit ${bucket} appends it to ${name(writer.physical)}.`,
        { read: from, wrote: [to], focus: [writer.physical] },
      );
      if (writer.next === b) {
        // A bucket claims its k-th input block only after k·b records were
        // read, so every record in that block has already moved.
        const read = offset + 1;
        if ((output + 1) * b > read) throw new Error("claimed an unread block");
        const claimed = scratchBlocks + firstBlock + output;
        const full = writer.physical;
        writers[bucket] = { physical: claimed, next: 0 };
        chain.push({ physical: claimed, len: 0 });
        output++;
        snap(
          "scatter",
          `${name(full)} is full. Bucket ${bucket} links the next input block, ` +
            `${name(claimed)}, which has been read completely.`,
          { focus: [full, claimed] },
        );
      }
    }
    shardChains.push(local);
    firstBlock += ownBlocks;
  }
  writers = null;

  // build_completion: one source per nonempty prefix, bucket-major and in
  // original shard order. Every other physical block is free.
  owners = Array<Owner>(blockCount).fill("free");
  const sources: { bucket: number; len: number }[] = [];
  const location: number[] = [];
  const bucketSources: number[][] = Array.from({ length: R }, () => []);
  const starts: number[] = [];
  let end = 0;
  for (let bucket = 0; bucket < R; bucket++) {
    starts.push(end);
    const joined: Prefix[] = [];
    for (const local of shardChains) {
      for (const prefix of local[bucket]) {
        if (prefix.len === 0) continue;
        owners[prefix.physical] = bucket;
        bucketSources[bucket].push(sources.length);
        sources.push({ bucket, len: prefix.len });
        location.push(prefix.physical);
        joined.push(prefix);
        end += prefix.len;
      }
    }
    chains[bucket] = joined;
  }
  if (end !== n) throw new Error(`ledger holds ${end} of ${n} records`);
  // One free cache per bucket, and the shared reserve last.
  const free: number[][] = Array.from({ length: R + 1 }, () => []);
  owners.forEach((owner, physical) => {
    if (owner === "free") free[R].push(physical);
  });
  const sourceOf = new Map<number, number>();
  location.forEach((physical, source) => {
    sourceOf.set(physical, source);
  });
  snap(
    "complete",
    `Every record has moved once. The ledger lists each bucket's prefixes in chain order: ` +
      chains.map((chain, k) => `bucket ${k} [${blocks(chain)}]`).join(", ") +
      ". Bucket k's final band starts where bucket k−1's ends.",
  );

  const transfers = { final: 0, relocation: 0 };
  const takeFree = (bucket: number) => {
    // The local cache, then the reserve, then the other buckets' caches.
    for (const cache of [
      bucket,
      R,
      ...Array.from({ length: R - 1 }, (_, i) => (bucket + i + 1) % R),
    ]) {
      while (free[cache].length) {
        const physical = free[cache].pop()!;
        // An entry turns stale once its block is final.
        if (owners![physical] === "free") return physical;
      }
    }
    throw new Error("no free block");
  };
  const protect = (physical: number, bucket: number) => {
    const owner = owners![physical];
    if (owner !== "free" && owner !== "final") {
      const target = takeFree(bucket);
      const source = sourceOf.get(physical)!;
      move(blockLoc(physical, 0), blockLoc(target, 0), sources[source].len);
      transfers.relocation += sources[source].len;
      owners![target] = owner;
      location[source] = target;
      sourceOf.delete(physical);
      sourceOf.set(target, source);
      const chain = chains[owner];
      chain[chain.findIndex((prefix) => prefix.physical === physical)].physical = target;
      owners![physical] = "final";
      snap(
        "complete",
        `${name(physical)} lies in bucket ${bucket}'s band but still holds a source of bucket ` +
          `${owner}. That source moves to the free block ${name(target)} first.`,
        { focus: [physical, target] },
      );
      return;
    }
    owners![physical] = "final";
  };
  // Protects every full block the band touches, except the source's own block.
  const protectBand = (start: number, len: number, source: number, bucket: number) => {
    let skipped = false;
    for (let position = start; position < start + len; ) {
      const block = Math.floor(position / b);
      if (block < fullBlocks) {
        const physical = scratchBlocks + block;
        if (physical === source) skipped = true;
        else protect(physical, bucket);
      }
      position += Math.min(b - (position % b), start + len - position);
    }
    return skipped;
  };

  for (const bucket of config.order ?? Array.from({ length: R }, (_, k) => k)) {
    let start = starts[bucket];
    for (const source of bucketSources[bucket]) {
      const { len } = sources[source];
      const initial = location[source];
      const skipped = protectBand(start, len, initial, bucket);
      // Protection may have relocated this source: resolve it only now.
      const physical = location[source];
      if (owners[physical] !== bucket) throw new Error("source owner changed");
      const wrote = Array.from({ length: len }, (_, i) => inputLoc(start + i));
      move(blockLoc(physical, 0), inputLoc(start), len);
      transfers.final += len;
      chains[bucket].shift();
      sourceOf.delete(physical);
      if (skipped && physical === initial) {
        // The source overlaps its own destination: one overlapping copy, and
        // the block holds no unread source afterwards.
        owners[physical] = "final";
      } else {
        owners[physical] = "free";
        free[bucket].push(physical);
      }
      snap(
        "complete",
        `Copy ${name(physical)}'s ${len} record${len === 1 ? "" : "s"} to positions ` +
          `${start}–${start + len - 1}` +
          (skipped && physical === initial
            ? `, overlapping the block itself, which becomes final.`
            : `; ${name(physical)} becomes free.`),
        { focus: [physical], wrote },
      );
      start += len;
    }
    snap(
      "complete",
      `Bucket ${bucket}'s band is final: its callback may sort it while it is in cache.`,
    );
  }
  snap(
    "done",
    "The input slice holds every record once, grouped by digit, each group in input order.",
  );

  return {
    frames,
    output: input.map((c) => {
      if (!c.live || !c.rec) throw new Error("completion left a hole");
      return c.rec;
    }),
    blockCount,
    scratchBlocks,
    fullBlocks,
    transfers,
  };
}
