// SPDX-License-Identifier: BSD-2-Clause
// `plan_digits` and `top_window` from `rust/tessera-sort/src/engine.rs`, on
// BigInt masks so that the explanation pages can plan keys of any width.

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

function popcount(value: bigint): number {
  return [...value.toString(2)].filter((c) => c === "1").length;
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

/** Packs varying bits into windows, highest first; LSD passes run the plan in reverse. */
export function planDigits(varying: bigint, width: number, bits: number): Digit[] {
  const top = bitLen(varying);
  let even: number | null = popcount(varying) === top ? Math.ceil(top / width) : null;
  let remaining = varying;
  const digits: Digit[] = [];
  while (remaining !== 0n) {
    const windowWidth = even !== null && even > 0 ? Math.ceil(bitLen(remaining) / even) : width;
    const { digit, window } = topWindow(remaining, windowWidth, bits);
    digits.push(digit);
    remaining &= ~window;
    if (even !== null) even = Math.max(0, even - 1);
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

/** Mirrors `sort_tile`'s leaf choice for one sampled top-digit fan-out. */
export function chooseTileLeaf(
  records: number,
  varying: bigint,
  tableWidth: number,
  sampledDistinct: number,
  bits: number,
): TileLeaf {
  if (records < 2 || varying === 0n) return "done";
  if (records <= 48) return "insertion";

  const passes = planDigits(varying, tableWidth, bits).length;
  if (passes <= 2) return "lsd";

  const sampleLen = Math.min(128, records);
  const distinct = Math.max(1, Math.min(sampledDistinct, sampleLen));
  if (distinct <= 2) return "msd";

  const { digit } = topWindow(varying, tableWidth, bits);
  const fanout = distinct >= sampleLen ? 2 ** digit.width : distinct;
  let largest = records;
  let depth = 0;
  while (largest > 48 && depth < passes) {
    depth++;
    largest = Math.floor(largest / fanout);
  }
  return Math.max(depth, 1) + 1 < passes ? "msd" : "lsd";
}
