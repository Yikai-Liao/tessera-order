// SPDX-License-Identifier: BSD-2-Clause
// `cost::plan_flat` and `Digit::top` from `rust/tessera-sort/src/cost.rs`, on
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

/** Packs varying bits into windows, highest first; a flat plan runs them in reverse. */
export function planFlat(varying: bigint, width: number, bits: number): Digit[] {
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

/** `table_bits`: the widest u16 cursor table that fits beside the tile. */
export function tableBits(records: number, recordBytes: number): number {
  const tileBytes = records * recordBytes;
  const widest = Math.floor(Math.log2(Math.max(1, Math.floor(tileBytes / 2))));
  // At most 8 buckets per record, or 4 for records wider than a machine word.
  const slack = recordBytes > 8 ? 2 : 3;
  const byRecords = Math.floor(Math.log2(Math.max(1, records))) + slack;
  return Math.min(14, Math.max(6, Math.min(widest, byRecords)));
}

export type TileRoute = "done" | "insertion" | "flat" | "split";

/**
 * Mirrors the cost model's tile choice for one sampled top-window fan-out.
 *
 * The model prices one counting pass per flat window against one counted split
 * that leaves children of `fanout` records, so a spread tile splits once and a
 * concentrated one finishes flat.
 */
export function chooseTileRoute(
  records: number,
  varying: bigint,
  tableWidth: number,
  sampledDistinct: number,
  bits: number,
): TileRoute {
  if (records < 2 || varying === 0n) return "done";
  if (records <= 48) return "insertion";

  const passes = planFlat(varying, tableWidth, bits).length;
  const { digit } = topWindow(varying, tableWidth, bits);
  const sampleLen = Math.min(128, records);
  const distinct = Math.max(1, Math.min(sampledDistinct, sampleLen));
  const spread = distinct >= sampleLen;
  const fanout = spread ? Math.min(2 ** digit.width, records) : distinct;

  // One counting pass per window against one split, its children and their
  // leaves; a pass costs a read and a move.
  const flat = 2 * passes + Math.max(0, passes - 1);
  let largest = records / Math.max(2, fanout);
  let depth = 1;
  while (largest > 48) {
    depth++;
    largest /= Math.max(2, fanout);
  }
  const split = 2 + depth * 0.5;
  return flat < split ? "flat" : "split";
}
