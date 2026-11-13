// SPDX-License-Identifier: BSD-2-Clause
// `plan_digits` and `top_window` from `rust/tessera-order/src/engine/plan.rs`, on
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

/** One or two flat digits, or one MSD digit when flat completion needs more. */
export function planDigits(varying: bigint, width: number, bits: number): Digit[] {
  if (varying === 0n) return [];
  const { digit: high, window } = topWindow(varying, width, bits);
  const remaining = varying & ~window;
  if (remaining === 0n) return [high];
  const low = topWindow(remaining, bits, bits).digit;
  if (low.width > width) return [high];
  let digits = [high, low];
  let slots = 2 ** high.width + 2 ** low.width;
  for (let highWidth = 1; highWidth < high.width; highWidth++) {
    const next = topWindow(varying, highWidth, bits);
    const lower = topWindow(varying & ~next.window, bits, bits).digit;
    const cost = 2 ** next.digit.width + 2 ** lower.width;
    if (lower.width <= width && cost < slots) {
      digits = [next.digit, lower];
      slots = cost;
    }
  }
  return digits;
}

export type TileLeaf = "done" | "insertion" | "msd" | "lsd";

/** Mirrors the bounded planner: finish at most two flat passes, otherwise split. */
export function chooseTileLeaf(
  records: number,
  varying: bigint,
  tableWidth: number,
  bits: number,
): TileLeaf {
  if (records < 2 || varying === 0n) return "done";
  if (records <= 48) return "insertion";
  const remaining = varying & ~topWindow(varying, tableWidth, bits).window;
  return remaining === 0n || topWindow(remaining, bits, bits).digit.width <= tableWidth
    ? "lsd"
    : "msd";
}
