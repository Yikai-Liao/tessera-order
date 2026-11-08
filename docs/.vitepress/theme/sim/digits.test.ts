// SPDX-License-Identifier: BSD-2-Clause
import { chooseTileLeaf, planDigits, tileDigitBits } from "./digits.ts";

const show = (v: bigint, w: number) =>
  planDigits(v, w, 128)
    .map((d) => `${d.shift}+${d.width}`)
    .join(" ");
const check = (got: string, want: string) => {
  if (got !== want) throw new Error(`${got} != ${want}`);
};
// The engine's doc comment: 32 contiguous bits at 14-bit tables become 11 + 11 + 10.
check(show((1n << 32n) - 1n, 14), "21+11 10+11 0+10");
// Gaps keep full-width windows that skip constant bits.
check(show(0xff00ffn, 8), "16+8 0+8");
for (const w of [6, 8, 11, 14]) {
  for (const v of [
    1n,
    1n << 127n,
    (1n << 127n) | 1n,
    (1n << 128n) - 1n,
    0x01010101010101010101010101010101n,
  ]) {
    let mask = 0n;
    for (const d of planDigits(v, w, 128)) {
      const bits = ((1n << BigInt(d.width)) - 1n) << BigInt(d.shift);
      if (mask & bits) throw new Error("overlap");
      if (d.width > w) throw new Error("too wide");
      mask |= bits;
    }
    if ((mask & v) !== v) throw new Error("uncovered bits");
  }
}
// `tile_digit_bits`: the table must fit beside the tile, and its buckets are capped at
// 8 per record, or 4 above 8 bytes. 2,048 16-byte records get 13 bits, not 14.
check(String(tileDigitBits(1024, 8)), "12");
check(String(tileDigitBits(1024, 16)), "12");
check(String(tileDigitBits(4096, 8)), "14");
check(String(tileDigitBits(2048, 16)), "13");

// A two-bucket sample predicts five MSD levels plus insertion, tying six LSD passes.
check(chooseTileLeaf(1000, (1n << 64n) - 1n, 11, 2, 64), "msd");
// One- and two-pass plans skip the sample, even when it reports two buckets.
check(chooseTileLeaf(1000, (1n << 20n) - 1n, 11, 2, 64), "lsd");
check(chooseTileLeaf(48, (1n << 64n) - 1n, 11, 2, 64), "insertion");
check(chooseTileLeaf(8192, 0n, 14, 2, 32), "done");
console.log("digit planner and tile routing models ok");
