// SPDX-License-Identifier: BSD-2-Clause
// Run with `pnpm test:sim`: the model must produce the stable order and
// conserve every record for every small configuration.
import { simulate } from "./blockPartition.ts";

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
console.log(`block partition model: ${cases} cases ok`);
