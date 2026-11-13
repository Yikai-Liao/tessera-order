// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import { command } from "../../tools/process.ts";
import { inventorySchema } from "../catalog.ts";
import { trackingAlgorithm, trackingAlgorithms, trackingBranch } from "./algorithms.ts";
import { trackingShards } from "./selection.ts";

const serial = "rust/u64/random/1000000/s1/api";

test("dispatch exposes every production algorithm and keeps auto tracking Tessera-only", async () => {
  const workflow = await readFile(
    new URL("../../.github/workflows/main-benchmarks.yml", import.meta.url),
    "utf8",
  );
  const options = [...workflow.matchAll(/^ {10}- ([a-z0-9-]+)$/gm)].map((match) => match[1]);
  assert.deepEqual(
    options.toSorted(),
    ["all", ...trackingAlgorithms.map((entry) => entry.path)].toSorted(),
  );
  assert.equal(new Set(options).size, options.length);
  const matrix = async (selection?: string) =>
    JSON.parse(
      await command(process.execPath, [new URL("./algorithms.ts", import.meta.url).pathname], {
        capture: true,
        env: { TRACKING_ALGORITHM: selection, GITHUB_OUTPUT: "" },
      }),
    );
  assert.deepEqual(await matrix(), { include: [{ algorithm: "tessera", native: false }] });
  const all = await matrix("all");
  assert.equal(all.include.length, trackingAlgorithms.length);
  assert.ok(
    all.include.some(
      (entry: { algorithm: string; native: boolean }) =>
        entry.algorithm === "ips4o" && entry.native,
    ),
  );
  assert.equal(trackingBranch("tessera"), "main");
  assert.equal(trackingBranch("rust-std-stable"), "algorithm-rust-std-stable");
  for (const id of ["../main", "std-stable", "tessera-baseline", "missing"])
    assert.throws(() => trackingAlgorithm(id));
});

test("executable inventories reject empty, overlapping or malformed pairs", () => {
  assert.deepEqual(inventorySchema.parse({ supported: [serial], unsupported: [] }).supported, [
    serial,
  ]);
  for (const input of [
    { supported: [], unsupported: [serial] },
    { supported: [serial], unsupported: [serial] },
    { supported: [serial, serial], unsupported: [] },
    { supported: ["not-a-case"], unsupported: [] },
  ])
    assert.throws(() => inventorySchema.parse(input));
});

test("complete-case shards preserve every budget and balance long and short cases deterministically", () => {
  const ids = Array.from({ length: 30 }, (_, i) => serial.replace("1000000", String(1000000 + i)));
  ids.push(...Array.from({ length: 30 }, (_, i) => serial.replace("1000000", String(i + 1))));
  const shards = trackingShards(ids);
  assert.ok(shards.length > 1);
  assert.deepEqual(shards.flat().toSorted(), ids.toSorted());
  assert.deepEqual(trackingShards(ids.toReversed()), shards);
  for (const shard of shards) assert.ok(shard.length > 0);
});
