// SPDX-License-Identifier: BSD-2-Clause
// Run with `pnpm docs:test`: the model must produce the stable order and
// conserve every record for every small configuration.
import { simulate } from "./blockPartition.ts";
import { topWindow } from "./digits.ts";

let seed = 12345;
const random = () => {
  seed = (seed * 1103515245 + 12345) % 2 ** 31;
  return seed / 2 ** 31;
};
let cases = 0;
for (const R of [2, 4, 8]) {
  for (let b = 1; b <= 6; b++) {
    for (let n = 2; n <= 60; n++) {
      for (let trial = 0; trial < 4; trial++) {
        const keys = Array.from({ length: n }, () => Math.floor(random() * R * 10));
        const classify = (key: number) => Math.floor(key / 10);
        const { output, frames } = simulate({ keys, buckets: R, blockSize: b, classify });
        const expected = keys
          .map((key, id) => ({ key, id, bucket: classify(key) }))
          .sort((x, y) => x.bucket - y.bucket || x.id - y.id);
        const got = output.map((r) => r.id).join();
        const want = expected.map((r) => r.id).join();
        if (got !== want) throw new Error(`R=${R} b=${b} n=${n}: ${got} != ${want}`);
        for (const frame of frames) {
          const live = [...frame.scratch, ...frame.input]
            .filter((c) => c.live)
            .map((c) => c.rec!.id);
          if (new Set(live).size !== n || live.length !== n) {
            throw new Error(
              `R=${R} b=${b} n=${n}: frame "${frame.caption}" holds ${live.length} live records`,
            );
          }
        }
        cases++;
      }
    }
  }
}
// Changing keys can leave all records below or above an absent equality
// pivot. A digit fallback must consume bits even if its bucket is unchanged.
for (const band of [0, 2]) {
  const keys = Array.from({ length: 33 }, (_, id) => id);
  const equal = simulate({ keys, buckets: 4, blockSize: 4, classify: () => band });
  if (equal.output.some((record, id) => record.id !== id || record.bucket !== band))
    throw new Error("unchanged equality band lost records");
  let candidate = 0xffffn;
  let passes = 0;
  while (candidate) {
    const { digit, window } = topWindow(candidate, 3, 16);
    const fallback = simulate({ keys, buckets: 2 ** digit.width, blockSize: 4, classify: () => 0 });
    if (fallback.output.some((record, id) => record.id !== id))
      throw new Error("digit fallback lost records");
    const remaining = candidate & ~window;
    if (remaining === candidate) throw new Error("digit fallback did not consume bits");
    candidate = remaining;
    if (++passes > 6) throw new Error("digit fallback did not terminate");
  }
}
// A stable digit partition can expose monotone children anywhere in the
// input. Confirming each child allows a return or a stable reversal without
// assuming a short end run or reversing equal-key identities.
for (const direction of ["ascending", "descending", "constant"]) {
  const keys = Array.from({ length: 48 }, (_, id) =>
    direction === "constant"
      ? 7
      : direction === "descending"
        ? 15 - Math.floor(id / 3)
        : Math.floor(id / 3),
  );
  keys.splice(16, 0, ...keys.splice(0, 8));
  const partition = simulate({ keys, buckets: 4, blockSize: 4, classify: (key) => key >> 2 });
  const output = [];
  for (let bucket = 0; bucket < 4; bucket++) {
    const child = partition.output.filter((record) => record.bucket === bucket);
    const ascending = child.every(
      (record, index) => index === 0 || child[index - 1].key <= record.key,
    );
    const descending = child.every(
      (record, index) => index === 0 || child[index - 1].key >= record.key,
    );
    if (ascending) output.push(...child);
    else if (descending) {
      child.reverse();
      for (let start = 0; start < child.length; ) {
        let end = start + 1;
        while (end < child.length && child[end].key === child[start].key) end++;
        child.splice(start, end - start, ...child.slice(start, end).reverse());
        start = end;
      }
      output.push(...child);
    } else output.push(...child.sort((a, b) => a.key - b.key));
  }
  const expected = keys.map((key, id) => ({ key, id })).sort((a, b) => a.key - b.key);
  if (output.map((record) => record.id).join() !== expected.map((record) => record.id).join())
    throw new Error("child-order routing changed stable record order");
}
console.log(`block partition model: ${cases} cases ok`);
