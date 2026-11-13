// SPDX-License-Identifier: BSD-2-Clause
// Runs the engine's own block partition, compiled to WebAssembly from
// `rust/tessera-trace`, and decodes its event log. The events and their meaning
// are owned by `Event` in `rust/tessera-order/src/trace.rs`; this file only
// mirrors the five-word encoding in `rust/tessera-trace/src/lib.rs`.

export type Event =
  | { kind: "begin"; len: number; blockSize: number; buckets: number; scratchBlocks: number }
  | { kind: "writer"; bucket: number; physical: number }
  | { kind: "scatter"; input: number; bucket: number; physical: number; slot: number }
  | { kind: "source"; bucket: number; physical: number; len: number }
  | { kind: "final"; physical: number }
  | { kind: "relocate"; source: number; from: number; to: number }
  | { kind: "transfer"; source: number; physical: number; start: number; overlapped: boolean }
  | { kind: "band"; bucket: number; start: number; len: number };

export interface Partition {
  events: Event[];
  /** The keys in the order the partition left them. */
  output: number[];
}

export interface Engine {
  /** Partitions `keys` into `2^radix` buckets by `⌊key / divisor⌋`, in blocks of `blockSize`. */
  partition(keys: readonly number[], radix: number, blockSize: number, divisor: number): Partition;
}

interface Exports {
  memory: WebAssembly.Memory;
  keys(len: number): number;
  partition(radix: number, blockSize: number, divisor: number): number;
  events(): number;
}

const WORDS = 5;

function decode(w: Uint32Array, at: number): Event {
  const [tag, a, b, c, d] = w.subarray(at, at + WORDS);
  switch (tag) {
    case 0:
      return { kind: "begin", len: a, blockSize: b, buckets: c, scratchBlocks: d };
    case 1:
      return { kind: "writer", bucket: a, physical: b };
    case 2:
      return { kind: "scatter", input: a, bucket: b, physical: c, slot: d };
    case 3:
      return { kind: "source", bucket: a, physical: b, len: c };
    case 4:
      return { kind: "final", physical: a };
    case 5:
      return { kind: "relocate", source: a, from: b, to: c };
    case 6:
      return { kind: "transfer", source: a, physical: b, start: c, overlapped: d !== 0 };
    case 7:
      return { kind: "band", bucket: a, start: b, len: c };
    default:
      throw new Error(`unknown event tag ${tag}`);
  }
}

/** Instantiates the engine from the bytes of `tessera_trace.wasm`. */
export async function instantiate(bytes: BufferSource): Promise<Engine> {
  const { instance } = await WebAssembly.instantiate(bytes);
  const wasm = instance.exports as unknown as Exports;
  return {
    partition(keys, radix, blockSize, divisor) {
      // Views are taken after each call: the call may grow memory.
      const input = wasm.keys(keys.length);
      new Uint32Array(wasm.memory.buffer, input, keys.length).set(keys);
      const words = wasm.partition(radix, blockSize, divisor);
      const log = new Uint32Array(wasm.memory.buffer, wasm.events(), words);
      const events: Event[] = [];
      for (let at = 0; at < words; at += WORDS) events.push(decode(log, at));
      const output = Array.from(new Uint32Array(wasm.memory.buffer, input, keys.length));
      return { events, output };
    },
  };
}
