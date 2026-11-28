// SPDX-License-Identifier: BSD-2-Clause

// Workload examples for the interactive explorer and its Markdown table.
// Routing rules belong to the linked explanation chapters; memory uses blockSize.ts.
import { blockPartitionMemory, formatBytesBinary, TILE_BYTES } from "./blockSize.ts";

interface Scenario {
  id: string;
  label: string;
  boxes: readonly string[];
  edges: readonly string[];
  runs: string;
  skips: string;
  memory: string;
}

const N = 10_000_000;
const u64Digit = blockPartitionMemory(N, 256, 8);
const u64Equality = blockPartitionMemory(N, 4, 8);
const row128Lsd = N * 128;

export const scenarios: Scenario[] = [
  {
    id: "random-u64",
    label: "10M random u64",
    boxes: ["probe", "insertion", "varying", "node", "digit", "childOrder", "leafM", "leafI"],
    edges: [
      "probe-insertion",
      "insertion-varying",
      "varying-node",
      "node-digit",
      "digit-loop",
      "child-return",
      "child-node",
      "node-tile-rail",
      "tile-leafM",
      "tile-leafI",
    ],
    runs: "Order probe rejects quickly, budgeted insertion aborts on a tile copy, varying-bit scan sees 64 bits, top-digit block partition splits to 256 bands, then tiles finish with Tile MSD / insertion.",
    skips: "Sorted/reversed/constant exits, equality partition, and bucket-order split.",
    memory: `${formatBytesBinary(u64Digit.totalBytes + TILE_BYTES)} (${formatBytesBinary(u64Digit.totalBytes)} block partition at b = ${u64Digit.blockSize} + ${formatBytesBinary(TILE_BYTES)} tile) vs ${formatBytesBinary(u64Digit.lsdBytes)} LSD buffer`,
  },
  {
    id: "nearly-sorted",
    label: "10M nearly sorted",
    boxes: ["probe", "insertion", "sorted"],
    edges: ["probe-insertion", "insertion-sorted"],
    runs: "Order probe sees an inversion; budgeted insertion estimates swaps on one tile copy and finishes the whole slice in linear time.",
    skips: "Varying-bit scan, all node block partitions, and all tile radix passes.",
    memory: `${formatBytesBinary(TILE_BYTES)} (one-tile key probe copy; no block scratch) vs ${formatBytesBinary(u64Digit.lsdBytes)} LSD buffer`,
  },
  {
    id: "reversed-ties",
    label: "10M reversed + ties",
    boxes: ["probe", "sorted"],
    edges: ["probe-sorted"],
    runs: "Order probe confirms descending order across all 10M keys, reverses the slice in place, and reverses each equal-key run back to keep stability.",
    skips: "Insertion, varying-bit scan, every block partition, and every tile pass.",
    memory: `0 B (no allocation) vs ${formatBytesBinary(u64Digit.lsdBytes)} LSD buffer`,
  },
  {
    id: "zipf",
    label: "10M Zipf / repeated key",
    boxes: [
      "probe",
      "insertion",
      "varying",
      "node",
      "mode",
      "childOrder",
      "childDone",
      "digit",
      "leafM",
      "leafL",
      "leafI",
    ],
    edges: [
      "probe-insertion",
      "insertion-varying",
      "varying-node",
      "node-mode",
      "mode-loop",
      "child-return",
      "child-done",
      "child-node",
      "node-digit",
      "node-tile-rail",
      "tile-leafM",
      "tile-leafL",
      "tile-leafI",
    ],
    runs: "A key sample spots a dominant key; equality partition isolates the equal band in one pass and routes only the remaining bands.",
    skips: "Re-scattering the dominant key across every remaining radix digit.",
    memory: `${formatBytesBinary(u64Equality.totalBytes)} for the 4-bucket equality split (up to ${formatBytesBinary(u64Digit.totalBytes + TILE_BYTES)} if a sub-band takes an 8-bit digit) vs ${formatBytesBinary(u64Digit.lsdBytes)} LSD buffer`,
  },
  {
    id: "narrow-20",
    label: "10M 20-bit narrow keys",
    boxes: ["probe", "insertion", "varying", "node", "digit", "childOrder", "leafL"],
    edges: [
      "probe-insertion",
      "insertion-varying",
      "varying-node",
      "node-digit",
      "digit-loop",
      "child-return",
      "child-node",
      "node-tile-rail",
      "tile-leafL",
    ],
    runs: "Varying-bit scan drops 44 constant high bits; an 8-bit root partition yields ~39k-record bands (>8,192-record u64 tile), so each band takes one more narrow node split before finishing in L2 tiles.",
    skips: "All 44 constant high key bits — zero empty radix passes over unused bits.",
    memory: `${formatBytesBinary(u64Digit.totalBytes + TILE_BYTES)} (${formatBytesBinary(u64Digit.totalBytes)} root block partition + ${formatBytesBinary(TILE_BYTES)} tile) vs ${formatBytesBinary(u64Digit.lsdBytes)} LSD buffer`,
  },
  {
    id: "argsort-128b",
    label: "10M argsort (128B rows)",
    boxes: [
      "probe",
      "insertion",
      "varying",
      "node",
      "indexScatter",
      "childOrder",
      "digit",
      "leafM",
      "leafL",
    ],
    edges: [
      "probe-insertion",
      "insertion-varying",
      "varying-node",
      "node-indexScatter",
      "indexScatter-loop",
      "child-return",
      "child-node",
      "node-digit",
      "digit-loop",
      "node-tile-rail",
      "tile-leafM",
      "tile-leafL",
    ],
    runs: "Serial argsort probes order, scans varying bits, scatters 64-bit indices into 256 bands with an 8-bit counting pass (packing the next key bits into each word), and refines dense index bands through the shared node/tile pipeline.",
    skips: `Moving any of the ${formatBytesBinary(row128Lsd)} of 128-byte source records; source lookups occur only on packed-bit ties.`,
    memory: `${formatBytesBinary(u64Digit.lsdBytes)} index output + ${formatBytesBinary(u64Digit.totalBytes + TILE_BYTES)} index workspace vs ${formatBytesBinary(row128Lsd)} full-record LSD buffer`,
  },
  {
    id: "select-median",
    label: "10M median select_nth",
    boxes: [
      "probe",
      "insertion",
      "varying",
      "node",
      "digit",
      "childOrder",
      "leafM",
      "leafL",
      "leafI",
    ],
    edges: [
      "probe-insertion",
      "insertion-varying",
      "varying-node",
      "node-digit",
      "digit-loop",
      "child-return",
      "child-node",
      "node-tile-rail",
      "tile-leafM",
      "tile-leafL",
      "tile-leafI",
    ],
    runs: "Root block partition completes every band stably, then descends only into the single bucket containing rank 5,000,000 until its tile finishes.",
    skips: "Refining every non-target bucket (~255/256 of uniform random keys after pass 1).",
    memory: `${formatBytesBinary(u64Digit.totalBytes + TILE_BYTES)} (no output allocation) vs ${formatBytesBinary(u64Digit.lsdBytes)} LSD buffer`,
  },
];
