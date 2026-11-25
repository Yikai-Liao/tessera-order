// SPDX-License-Identifier: BSD-2-Clause
// `plan_digits` and `top_window` from `rust/tessera-order/src/engine/tile/plan.rs`, on
// BigInt masks so that the explanation pages can plan keys of any width.

/** One key per equal stratum at a golden-ratio offset, mirroring `KeySample::collect`. */
export function sampleKeys(records: readonly bigint[]): bigint[] {
  if (records.length === 0) return [];
  const len = Math.min(records.length, 128);
  const stride = Math.floor(records.length / len);
  return Array.from({ length: len }, (_, stratum) => {
    const fraction = (BigInt(stratum) * 0x9e3779b97f4a7c15n) & ((1n << 64n) - 1n);
    const offset = Number((fraction * BigInt(stride)) >> 64n);
    return records[stratum * stride + offset];
  }).sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
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

export type TileLeaf = "done" | "insertion" | "msd" | "lsd";

export interface TileDecision {
  leaf: TileLeaf;
  digits: Digit[];
  simdMsdOverride: boolean;
}

const SIMD_MIN = 64;

/** Shared tile-leaf and digit decision mirroring `engine/tile/plan.rs` and `Scratch::sort`. */
export function planTile(
  records: number,
  varying: bigint,
  tableWidth: number,
  bits: number,
  recordBytes = bits / 8,
  simdMsd = false,
): TileDecision {
  if (records < 2 || varying === 0n) return { leaf: "done", digits: [], simdMsdOverride: false };
  const insertionLimit = Math.max(16, Math.floor((48 * 16) / (16 + recordBytes)));
  if (records <= insertionLimit) return { leaf: "insertion", digits: [], simdMsdOverride: false };
  const { digit: high, window } = topWindow(varying, tableWidth, bits);
  const remaining = varying & ~window;
  if (remaining === 0n) return { leaf: "lsd", digits: [high], simdMsdOverride: false };
  const low = topWindow(remaining, bits, bits).digit;
  if (low.width > tableWidth) return { leaf: "msd", digits: [high], simdMsdOverride: false };
  let digits = [high, low];
  let slots = 2 ** high.width + 2 ** low.width;
  for (let highWidth = 1; highWidth < high.width; highWidth++) {
    const next = topWindow(varying, highWidth, bits);
    const lower = topWindow(varying & ~next.window, bits, bits).digit;
    const cost = 2 ** next.digit.width + 2 ** lower.width;
    if (lower.width <= tableWidth && cost < slots) {
      digits = [next.digit, lower];
      slots = cost;
    }
  }
  const msdSlots = 2 ** high.width;
  if (simdMsd && records >= SIMD_MIN && msdSlots >= records && slots > msdSlots) {
    return { leaf: "msd", digits: [high], simdMsdOverride: true };
  }
  return { leaf: "lsd", digits, simdMsdOverride: false };
}

/** One or two flat digits, or one MSD digit when flat completion needs more (or when SIMD MSD finishing dominates a two-pass split). */
export function planDigits(
  varying: bigint,
  width: number,
  bits: number,
  len = Number.POSITIVE_INFINITY,
  simdMsd = false,
): Digit[] {
  return planTile(len, varying, width, bits, bits / 8, simdMsd).digits;
}

/** Mirrors the bounded planner: finish at most two flat passes, otherwise split. */
export function chooseTileLeaf(
  records: number,
  varying: bigint,
  tableWidth: number,
  bits: number,
  recordBytes = bits / 8,
  simdMsd = false,
): TileLeaf {
  return planTile(records, varying, tableWidth, bits, recordBytes, simdMsd).leaf;
}
