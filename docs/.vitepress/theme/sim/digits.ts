// SPDX-License-Identifier: BSD-2-Clause
// `plan_digits` and `top_window` from `rust/tessera-sort/src/engine/plan.rs`, on
// BigInt masks so that the explanation pages can plan keys of any width.

/** Bounded clustered sample shared by the engine's mode and occupancy decisions. */
export function sampleKeys(records: readonly bigint[]): bigint[] {
  if (records.length === 0) return [];
  const len = Math.min(records.length, 128);
  const runs = Math.ceil(len / 8);
  const stride = 8 + Math.floor((records.length - len) / runs);
  return Array.from(
    { length: len },
    (_, index) => records[Math.floor(index / 8) * stride + (index % 8)],
  ).sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
}

export interface Digit {
  shift: number;
  width: number;
}

export function bitLen(value: bigint): number {
  return value === 0n ? 0 : value.toString(2).length;
}

function trailingZeros(value: bigint): number {
  let count = 0;
  while (((value >> BigInt(count)) & 1n) === 0n) count++;
  return count;
}

/** The window of `width` bits under the highest remaining bit, snapped to its lowest varying bit. */
export function topWindow(
  remaining: bigint,
  width: number,
  bits: number,
): { digit: Digit; window: bigint } {
  const high = bitLen(remaining) - 1;
  const full = (1n << BigInt(bits)) - 1n;
  const window = (full << BigInt(Math.max(0, high + 1 - width))) & full;
  const low = trailingZeros(remaining & window);
  return { digit: { shift: low, width: high - low + 1 }, window };
}

/** Minimum-pass plan, minimizing bucket slots among plans with that pass count. */
export function planDigits(varying: bigint, width: number, _bits: number): Digit[] {
  const positions: number[] = [];
  for (let bit = 0; bit < bitLen(varying); bit++) {
    if ((varying & (1n << BigInt(bit))) !== 0n) positions.push(bit);
  }
  const cost = Array.from({ length: positions.length + 1 }, () => [0, 0]);
  const previous = new Array<number>(positions.length + 1).fill(0);
  for (let end = 1; end <= positions.length; end++) {
    cost[end] = [Infinity, Infinity];
    for (let start = end - 1; start >= 0; start--) {
      const span = positions[end - 1] - positions[start] + 1;
      if (span > width) break;
      const candidate = [cost[start][0] + 1, cost[start][1] + 2 ** span];
      if (
        candidate[0] < cost[end][0] ||
        (candidate[0] === cost[end][0] && candidate[1] < cost[end][1])
      ) {
        cost[end] = candidate;
        previous[end] = start;
      }
    }
  }
  const digits: Digit[] = [];
  for (let end = positions.length; end > 0; end = previous[end]) {
    const start = previous[end];
    digits.push({ shift: positions[start], width: positions[end - 1] - positions[start] + 1 });
  }
  return digits;
}

/** `tile_digit_bits`: the widest u16 cursor table that fits beside the tile. */
export function tileDigitBits(records: number, recordBytes: number): number {
  const tileBytes = records * recordBytes;
  const widest = Math.floor(Math.log2(Math.max(1, Math.floor(tileBytes / 2))));
  // At most 8 buckets per record, or 4 for records wider than a machine word.
  const slack = recordBytes > 8 ? 2 : 3;
  const byRecords = Math.floor(Math.log2(Math.max(1, records))) + slack;
  return Math.min(14, Math.max(6, Math.min(widest, byRecords)));
}

export type TileLeaf = "done" | "insertion" | "msd" | "lsd";

/** Greedy count of width-bounded windows, without constructing a flat plan. */
function digitPasses(varying: bigint, width: number, bits: number): number {
  let passes = 0;
  while (varying !== 0n) {
    varying &= ~topWindow(varying, width, bits).window;
    passes++;
  }
  return passes;
}

/** One partition and conditional child work, in sampled-record pass units. */
export function partitionWork(
  records: number,
  varying: bigint,
  digit: Digit,
  sample: readonly bigint[],
  recordBytes: number,
  bits: number,
): number {
  const mask = ((1n << BigInt(digit.width)) - 1n) << BigInt(digit.shift);
  const remaining = varying & ~mask;
  const keys = [...sample].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
  const singletonLen = Math.ceil(records / 2 ** digit.width);
  const singletonPasses =
    singletonLen <= 48 ? 0 : digitPasses(remaining, tileDigitBits(singletonLen, recordBytes), bits);
  let work = 2 * keys.length;
  for (let start = 0; start < keys.length; ) {
    let end = start + 1;
    while (end < keys.length && (keys[end] & mask) === (keys[start] & mask)) end++;
    const len = end - start;
    const childLen = Math.ceil((records * len) / keys.length);
    if (len === 1) {
      work += singletonPasses;
    } else if (childLen > 48 && keys[start] !== keys[end - 1]) {
      let childVarying = 0n;
      for (let index = start; index < end; index++) childVarying |= keys[index] ^ keys[start];
      work +=
        len * digitPasses(childVarying & remaining, tileDigitBits(childLen, recordBytes), bits);
    }
    start = end;
  }
  return work;
}

/** Mirrors the planner's weighted child-work comparison; samples are advisory. */
export function chooseTileLeaf(
  records: number,
  varying: bigint,
  tableWidth: number,
  sample: readonly bigint[],
  bits: number,
  recordBytes = 8,
): TileLeaf {
  if (records < 2 || varying === 0n) return "done";
  if (records <= 48) return "insertion";
  const passes = digitPasses(varying, tableWidth, bits);
  if (passes <= 2) return "lsd";
  const { digit } = topWindow(varying, tableWidth, bits);
  return partitionWork(records, varying, digit, sample, recordBytes, bits) < sample.length * passes
    ? "msd"
    : "lsd";
}
