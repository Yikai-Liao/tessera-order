// SPDX-License-Identifier: BSD-2-Clause
// Replays one stable block partition that the engine itself ran (see
// `engine.ts`) as frames for the block-partition figure. The engine decides
// every step; this file only applies each event to a picture of the scratch
// and input cells and describes it. It moves record identities instead of
// bytes and checks that its picture ends where the engine's slice does.

import type { Engine, Event } from "./engine.ts";

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
  /** Digit width: the partition has 2^radix buckets. */
  radix: number;
  blockSize: number;
  /** A key's bucket is ⌊key / divisor⌋. */
  divisor: number;
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

export function simulate(engine: Engine, config: Config): Result {
  const { keys, blockSize: b } = config;
  const { events, output: engineOutput } = engine.partition(keys, config.radix, b, config.divisor);
  const begin = events[0];
  if (begin?.kind !== "begin") throw new Error("the log does not start a partition");
  const { len: n, buckets: R, scratchBlocks } = begin;
  const fullBlocks = Math.floor(n / b);
  const blockCount = scratchBlocks + fullBlocks;
  const name = (p: number) => blockName(p, scratchBlocks);
  const blocks = (list: Prefix[]) => list.map((prefix) => name(prefix.physical)).join(", ");

  // The engine reports each record's bucket as it scatters it.
  const bucketOf: number[] = [];
  for (const event of events) if (event.kind === "scatter") bucketOf[event.input] = event.bucket;
  const scratch: Cell[] = Array.from({ length: scratchBlocks * b }, () => ({
    rec: null,
    live: false,
  }));
  const input: Cell[] = keys.map((key, id) => ({
    rec: { id, key, bucket: bucketOf[id] },
    live: true,
  }));
  const blockLoc = (physical: number, offset: number): Loc =>
    physical < scratchBlocks
      ? { region: "scratch", index: physical * b + offset }
      : { region: "input", index: (physical - scratchBlocks) * b + offset };
  const inputLoc = (index: number): Loc => ({ region: "input", index });
  const cells = (region: Region) => (region === "scratch" ? scratch : input);

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
      `and a tail of ${n % b}; scratch holds 2R = ${scratchBlocks} blocks.`,
  );

  const sources: { bucket: number; len: number; physical: number }[] = [];
  let transferred = 0;
  const transfers = { final: 0, relocation: 0 };
  // Frames that summarise several events are taken once the run ends.
  const pending = { writers: false, ledger: false };
  const flush = () => {
    if (pending.writers && writers) {
      snap(
        "scatter",
        `Each bucket starts its chain in a scratch block: ` +
          writers.map((w, k) => `bucket ${k} → ${name(w.physical)}`).join(", ") +
          `. Blocks ${name(R)}–${name(scratchBlocks - 1)} stay free.`,
        { focus: writers.map((w) => w.physical) },
      );
    }
    if (pending.ledger) {
      snap(
        "complete",
        `Every record has moved once. The ledger lists each bucket's prefixes in chain order: ` +
          chains.map((chain, k) => `bucket ${k} [${blocks(chain)}]`).join(", ") +
          ". Bucket k's final band starts where bucket k−1's ends.",
      );
    }
    pending.writers = false;
    pending.ledger = false;
  };

  for (const event of events.slice(1)) {
    if (event.kind !== "writer" && event.kind !== "source") flush();
    apply(event);
  }
  flush();
  snap(
    "done",
    "The input slice holds every record once, grouped by digit, each group in input order.",
  );

  function apply(event: Event) {
    switch (event.kind) {
      case "begin":
        throw new Error("one partition per log");
      case "writer": {
        if (!writers) {
          writers = [];
          pending.writers = true;
        }
        const full = writers[event.bucket];
        writers[event.bucket] = { physical: event.physical };
        chains[event.bucket].push({ physical: event.physical, len: 0 });
        if (full) {
          snap(
            "scatter",
            `${name(full.physical)} is full. Bucket ${event.bucket} links the next input block, ` +
              `${name(event.physical)}, which has been read completely.`,
            { focus: [full.physical, event.physical] },
          );
        }
        return;
      }
      case "scatter": {
        const from = inputLoc(event.input);
        const to = blockLoc(event.physical, event.slot);
        const key = input[event.input].rec!.key;
        move(from, to, 1);
        const chain = chains[event.bucket];
        chain[chain.length - 1].len = event.slot + 1;
        snap(
          "scatter",
          `Read key ${key}; digit ${event.bucket} appends it to ${name(event.physical)}.`,
          { read: from, wrote: [to], focus: [event.physical] },
        );
        return;
      }
      case "source": {
        if (!owners) {
          // The ledger replaces the scatter chains: every block without a source is free.
          writers = null;
          owners = Array<Owner>(blockCount).fill("free");
          for (const chain of chains) chain.length = 0;
          pending.ledger = true;
        }
        owners[event.physical] = event.bucket;
        sources.push({ ...event });
        chains[event.bucket].push({ physical: event.physical, len: event.len });
        return;
      }
      case "final":
        owners![event.physical] = "final";
        return;
      case "relocate": {
        const source = sources[event.source];
        move(blockLoc(event.from, 0), blockLoc(event.to, 0), source.len);
        transfers.relocation += source.len;
        source.physical = event.to;
        owners![event.to] = source.bucket;
        owners![event.from] = "final";
        const chain = chains[source.bucket];
        chain[chain.findIndex((prefix) => prefix.physical === event.from)].physical = event.to;
        // Serial completion transfers sources in ledger order.
        const completing = sources[transferred].bucket;
        snap(
          "complete",
          `${name(event.from)} lies in bucket ${completing}'s band but still holds a source of ` +
            `bucket ${source.bucket}. That source moves to the free block ${name(event.to)} first.`,
          { focus: [event.from, event.to] },
        );
        return;
      }
      case "transfer": {
        const source = sources[event.source];
        const { len } = source;
        const wrote = Array.from({ length: len }, (_, i) => inputLoc(event.start + i));
        move(blockLoc(event.physical, 0), inputLoc(event.start), len);
        transfers.final += len;
        transferred++;
        chains[source.bucket].shift();
        owners![event.physical] = event.overlapped ? "final" : "free";
        snap(
          "complete",
          `Copy ${name(event.physical)}'s ${len} record${len === 1 ? "" : "s"} to positions ` +
            `${event.start}–${event.start + len - 1}` +
            (event.overlapped
              ? `, overlapping the block itself, which becomes final.`
              : `; ${name(event.physical)} becomes free.`),
          { focus: [event.physical], wrote },
        );
        return;
      }
      case "band":
        snap(
          "complete",
          `Bucket ${event.bucket}'s band is final: its callback may sort it while it is in cache.`,
        );
        return;
    }
  }

  const output = input.map((c) => {
    if (!c.live || !c.rec) throw new Error("completion left a hole");
    return c.rec;
  });
  if (output.some((rec, i) => rec.key !== engineOutput[i])) {
    throw new Error("the replay disagrees with the engine's output");
  }
  return { frames, output, blockCount, scratchBlocks, fullBlocks, transfers };
}
