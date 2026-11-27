<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { useRouter, withBase } from "vitepress";
import { computed, ref } from "vue";
import { blockPartitionMemory, formatBytesBinary, TILE_BYTES } from "../sim/blockSize.ts";

// Interactive route & scale explorer for `engine/refine.rs`. Every box links to
// the chapter that explains it; boxes navigate with the router instead of SVG
// `<a>` elements, whose `href` VitePress's link prefetcher cannot read.
interface Box {
  id: string;
  x: number;
  y: number;
  w: number;
  title: string;
  detail: string;
  link: string;
  kind?: "exit" | "node" | "tile";
}

interface Edge {
  id: string;
  d: string;
  marker?: "default" | "node";
  dashed?: boolean;
}

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

const boxes: Box[] = [
  { id: "probe", x: 200, y: 10, w: 280, title: "Order probe", detail: "unit stride: adjacent scan; otherwise sample", link: "/explain/routing#order-probe" },
  { id: "sorted", x: 530, y: 10, w: 160, title: "Done", detail: "or reverse stably", link: "/explain/routing#order-probe", kind: "exit" },
  { id: "insertion", x: 200, y: 80, w: 280, title: "Insertion", detail: "finish small tiles · budget larger inputs", link: "/explain/routing#budgeted-insertion" },
  { id: "varying", x: 200, y: 150, w: 280, title: "Varying bits", detail: "drop bits equal in every key", link: "/explain/keys#varying-bits" },
  { id: "constant", x: 530, y: 150, w: 160, title: "Done", detail: "all keys equal", link: "/explain/keys#varying-bits", kind: "exit" },
  { id: "childOrder", x: 200, y: 230, w: 280, title: "Confirm child order", detail: "confirm first; scan variation if needed", link: "/explain/routing#nodes-and-tiles" },
  { id: "childDone", x: 530, y: 230, w: 160, title: "Done", detail: "or reverse stably", link: "/explain/routing#nodes-and-tiles", kind: "exit" },
  { id: "node", x: 200, y: 300, w: 280, title: "Larger than a tile?", detail: "64 KiB of records", link: "/explain/routing#nodes-and-tiles" },
  { id: "mode", x: 30, y: 390, w: 190, title: "Repeated key", detail: "equality partition", link: "/explain/routing#repeated-key", kind: "node" },
  { id: "bucket", x: 260, y: 390, w: 190, title: "Bucket order", detail: "digit route: split where order is", link: "/explain/routing#already-in-bucket-order", kind: "node" },
  { id: "digit", x: 490, y: 390, w: 190, title: "Top digit", detail: "stable block partition", link: "/explain/block-partition", kind: "node" },
  { id: "leafI", x: 30, y: 526, w: 190, title: "Insertion sort", detail: "limit depends on record width", link: "/explain/tiles#insertion", kind: "tile" },
  { id: "leafM", x: 260, y: 526, w: 190, title: "Tile MSD", detail: "partition, then recurse", link: "/explain/tiles#tile-msd", kind: "tile" },
  { id: "leafL", x: 490, y: 526, w: 190, title: "Tile LSD", detail: "counting passes in scratch", link: "/explain/tiles#tile-lsd", kind: "tile" },
];

const edges: Edge[] = [
  { id: "probe-sorted", d: "M480,33 H526", marker: "default" },
  { id: "probe-insertion", d: "M340,56 V76", marker: "default" },
  { id: "insertion-sorted", d: "M480,103 C505,103 505,62 526,48", marker: "default" },
  { id: "insertion-varying", d: "M340,126 V146", marker: "default" },
  { id: "varying-constant", d: "M480,173 H526", marker: "default" },
  { id: "varying-node", d: "M340,196 H180 V323 H196", marker: "default" },
  { id: "child-done", d: "M480,253 H526", marker: "default" },
  { id: "child-node", d: "M340,276 V296", marker: "default" },
  { id: "node-mode", d: "M280,346 C280,368 125,364 125,386", marker: "default" },
  { id: "node-bucket", d: "M340,346 V386", marker: "default" },
  { id: "node-digit", d: "M400,346 C400,368 585,364 585,386", marker: "default" },
  { id: "node-tile-rail", d: "M480,323 H690 V498 H125" },
  { id: "tile-leafI", d: "M125,498 V522", marker: "default" },
  { id: "tile-leafM", d: "M355,498 V522", marker: "default" },
  { id: "tile-leafL", d: "M585,498 V522", marker: "default" },
  { id: "mode-loop", d: "M125,436 V462", dashed: true },
  { id: "bucket-loop", d: "M355,436 V462", dashed: true },
  { id: "digit-loop", d: "M585,436 V462", dashed: true },
  { id: "child-return", d: "M585,462 H12 V253 H196", marker: "node", dashed: true },
];

const scenarios: Scenario[] = [
  {
    id: "random-u64",
    label: "10M random u64",
    boxes: ["probe", "insertion", "varying", "node", "digit", "childOrder", "leafM", "leafI"],
    edges: [
      "probe-insertion", "insertion-varying", "varying-node", "node-digit",
      "digit-loop", "child-return", "child-node", "node-tile-rail", "tile-leafM", "tile-leafI",
    ],
    runs: "Order probe rejects quickly, budgeted insertion aborts on a tile copy, varying-bit scan sees 64 bits, top-digit block partition splits to 256 bands, then tiles finish with Tile MSD / insertion.",
    skips: "Sorted/reversed/constant exits, equality partition, and bucket-order split.",
    memory: `${formatBytesBinary(u64Digit.totalBytes + TILE_BYTES)} (${formatBytesBinary(u64Digit.totalBytes)} block partition at b = ${u64Digit.blockSize} + 64 KiB tile) vs ${formatBytesBinary(u64Digit.lsdBytes)} LSD buffer`,
  },
  {
    id: "nearly-sorted",
    label: "10M nearly sorted",
    boxes: ["probe", "insertion", "sorted"],
    edges: ["probe-insertion", "insertion-sorted"],
    runs: "Order probe sees an inversion; budgeted insertion estimates swaps on one tile copy and finishes the whole slice in linear time.",
    skips: "Varying-bit scan, all node block partitions, and all tile radix passes.",
    memory: `64 KiB (one-tile key probe copy; no block scratch) vs ${formatBytesBinary(u64Digit.lsdBytes)} LSD buffer`,
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
    boxes: ["probe", "insertion", "varying", "node", "mode", "childOrder", "childDone", "digit", "leafM", "leafL", "leafI"],
    edges: [
      "probe-insertion", "insertion-varying", "varying-node", "node-mode",
      "mode-loop", "child-return", "child-done", "child-node", "node-digit",
      "node-tile-rail", "tile-leafM", "tile-leafL", "tile-leafI",
    ],
    runs: "128-key sample spots a frequent key (>= 25%); equality partition isolates the equal band in one pass and routes only the remaining bands.",
    skips: "Re-scattering the dominant key across every remaining radix digit.",
    memory: `${formatBytesBinary(u64Equality.totalBytes)} for the 4-bucket equality split (up to ${formatBytesBinary(u64Digit.totalBytes + TILE_BYTES)} if a sub-band takes an 8-bit digit) vs ${formatBytesBinary(u64Digit.lsdBytes)} LSD buffer`,
  },
  {
    id: "narrow-20",
    label: "10M 20-bit narrow keys",
    boxes: ["probe", "insertion", "varying", "node", "digit", "childOrder", "leafL"],
    edges: [
      "probe-insertion", "insertion-varying", "varying-node", "node-digit",
      "digit-loop", "child-return", "child-node", "node-tile-rail", "tile-leafL",
    ],
    runs: "Varying-bit scan drops 44 constant high bits; one 8-bit block partition leaves 12 bits, which finish inside each tile via flat Tile LSD.",
    skips: "44 constant high key bits (no empty passes) and multi-level MSD recursion.",
    memory: `${formatBytesBinary(u64Digit.totalBytes + TILE_BYTES)} (${formatBytesBinary(u64Digit.totalBytes)} block partition + 64 KiB tile) vs ${formatBytesBinary(u64Digit.lsdBytes)} LSD buffer`,
  },
  {
    id: "argsort-128b",
    label: "10M argsort (128B rows)",
    boxes: ["probe", "insertion", "varying", "node", "digit", "childOrder", "leafM", "leafL"],
    edges: [
      "probe-insertion", "insertion-varying", "varying-node", "node-digit",
      "digit-loop", "child-return", "child-node", "node-tile-rail", "tile-leafM", "tile-leafL",
    ],
    runs: "Counting pass splits 64-bit indices into 256 bands and packs the next key bits into each index word so dense bands sort integers directly.",
    skips: `Moving any of the ${formatBytesBinary(row128Lsd)} of 128-byte source records; source lookups occur only on packed-bit ties.`,
    memory: `${formatBytesBinary(u64Digit.lsdBytes)} index output + ${formatBytesBinary(u64Digit.totalBytes + TILE_BYTES)} index workspace vs ${formatBytesBinary(row128Lsd)} full-record LSD buffer`,
  },
  {
    id: "select-median",
    label: "10M median select_nth",
    boxes: ["probe", "varying", "node", "digit", "childOrder", "leafM", "leafL", "leafI"],
    edges: [
      "probe-insertion", "insertion-varying", "varying-node", "node-digit",
      "digit-loop", "child-return", "child-node", "node-tile-rail", "tile-leafM", "tile-leafL", "tile-leafI",
    ],
    runs: "Root block partition completes every band stably, then descends only into the single bucket containing rank 5,000,000 until its tile finishes.",
    skips: "Refining all ~255 non-target buckets (over 99% of the slice after pass 1).",
    memory: `${formatBytesBinary(u64Digit.totalBytes + TILE_BYTES)} (no output allocation) vs ${formatBytesBinary(u64Digit.lsdBytes)} LSD buffer`,
  },
];

const selectedId = ref<string>("random-u64");
const active = computed(() => scenarios.find((s) => s.id === selectedId.value) ?? null);
const activeBoxes = computed(() => (active.value ? new Set(active.value.boxes) : null));
const activeEdges = computed(() => (active.value ? new Set(active.value.edges) : null));

const H = 46;
const stroke = (b: Box) =>
  b.kind === "exit"
    ? "#1baf7a"
    : b.kind === "node"
      ? "#5CA4F8"
      : b.kind === "tile"
        ? "#B482F8"
        : "var(--vp-c-text-3)";

const router = useRouter();
const go = (link: string) => router.go(withBase(link));
</script>

<template>
  <figure class="tsr-figure tsr-pipeline">
    <fieldset class="tsr-scenarios" aria-label="Workload scenarios">
      <button
        v-for="s in scenarios"
        :key="s.id"
        type="button"
        class="tsr-scenario-btn"
        :class="{ active: selectedId === s.id }"
        :aria-pressed="selectedId === s.id"
        @click="selectedId = s.id"
      >
        {{ s.label }}
      </button>
      <button
        type="button"
        class="tsr-scenario-btn"
        :class="{ active: selectedId === 'all' }"
        :aria-pressed="selectedId === 'all'"
        @click="selectedId = 'all'"
      >
        All routes
      </button>
    </fieldset>

    <div v-if="active" class="tsr-route-readout" aria-live="polite">
      <div><strong>Runs:</strong> {{ active.runs }}</div>
      <div><strong>Skips:</strong> {{ active.skips }}</div>
      <div><strong>Extra memory:</strong> {{ active.memory }}</div>
    </div>

    <svg viewBox="0 0 700 586" width="700" role="img" aria-label="Tessera's pipeline from order probe to tile leaves">
      <defs>
        <marker id="tsr-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
          <path d="M0,0 L10,5 L0,10 z" fill="var(--vp-c-text-3)" />
        </marker>
        <marker id="tsr-arrow-node" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
          <path d="M0,0 L10,5 L0,10 z" fill="#5CA4F8" />
        </marker>
        <marker id="tsr-arrow-active" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
          <path d="M0,0 L10,5 L0,10 z" fill="var(--vp-c-brand-1)" />
        </marker>
      </defs>

      <g fill="none">
        <path
          v-for="edge in edges"
          :key="edge.id"
          :d="edge.d"
          :stroke="
            activeEdges?.has(edge.id)
              ? 'var(--vp-c-brand-1)'
              : edge.dashed
                ? '#5CA4F8'
                : 'var(--vp-c-text-3)'
          "
          :stroke-width="activeEdges?.has(edge.id) ? 2.2 : 1.5"
          :stroke-dasharray="edge.dashed ? '5 4' : undefined"
          :opacity="activeEdges ? (activeEdges.has(edge.id) ? 1 : 0.18) : 1"
          :marker-end="
            edge.marker
              ? activeEdges?.has(edge.id)
                ? 'url(#tsr-arrow-active)'
                : edge.marker === 'node'
                  ? 'url(#tsr-arrow-node)'
                  : 'url(#tsr-arrow)'
              : undefined
          "
        />
      </g>

      <g font-size="10" style="fill: var(--vp-c-text-2)" :opacity="active ? 0.65 : 1">
        <text x="486" y="26">ordered</text>
        <text x="488" y="84">sorted</text>
        <text x="486" y="166">none</text>
        <text x="486" y="246">ordered</text>
        <text x="520" y="316">no: a tile</text>
        <text x="490" y="342">yes: a node</text>
        <text x="236" y="478">larger children are routed again</text>
        <text x="140" y="492">the cost model picks one leaf</text>
      </g>

      <!-- biome-ignore lint/a11y/useSemanticElements: SVG `<a>` breaks VitePress prefetching (see the script comment). -->
      <g
        v-for="b in boxes"
        :key="b.id"
        class="tsr-link"
        :class="{ 'is-dim': activeBoxes && !activeBoxes.has(b.id), 'is-active': activeBoxes?.has(b.id) }"
        role="link"
        tabindex="0"
        :aria-label="`${b.title}: ${b.detail}`"
        @click="go(b.link)"
        @keydown.enter="go(b.link)"
      >
        <rect
          :x="b.x"
          :y="b.y"
          :width="b.w"
          :height="H"
          rx="8"
          :fill="activeBoxes?.has(b.id) ? 'var(--vp-c-brand-soft)' : 'var(--vp-c-bg)'"
          :stroke="stroke(b)"
          :stroke-width="activeBoxes?.has(b.id) ? 2.5 : 2"
        />
        <text :x="b.x + b.w / 2" :y="b.y + 20" text-anchor="middle" font-size="13" font-weight="600">{{ b.title }}</text>
        <text :x="b.x + b.w / 2" :y="b.y + 36" text-anchor="middle" font-size="10" style="fill: var(--vp-c-text-2)">{{ b.detail }}</text>
      </g>
    </svg>
    <div class="tsr-legend">
      <span><span class="swatch" style="border: 2px solid #1baf7a"></span>early exit</span>
      <span><span class="swatch" style="border: 2px solid #5CA4F8"></span>node partitions (O(√n) scratch)</span>
      <span><span class="swatch" style="border: 2px solid #B482F8"></span>tile leaves (64 KiB scratch)</span>
      <span>Click any box to open its chapter.</span>
    </div>
  </figure>
</template>

<style scoped>
.tsr-scenarios {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin: 0 0 12px;
  padding: 0;
  border: 0;
}
.tsr-scenario-btn {
  padding: 4px 10px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 999px;
  background: var(--vp-c-bg);
  color: var(--vp-c-text-2);
  font-size: 12.5px;
  font-weight: 500;
  cursor: pointer;
  transition: border-color 0.15s ease, color 0.15s ease, background-color 0.15s ease;
}
.tsr-scenario-btn:hover {
  border-color: var(--vp-c-brand-1);
  color: var(--vp-c-text-1);
}
.tsr-scenario-btn.active {
  border-color: var(--vp-c-brand-1);
  background: var(--vp-c-brand-soft);
  color: var(--vp-c-brand-1);
  font-weight: 600;
}
.tsr-route-readout {
  display: grid;
  gap: 4px;
  margin-bottom: 14px;
  padding: 10px 14px;
  border-left: 3px solid var(--vp-c-brand-1);
  border-radius: 6px;
  background: var(--vp-c-bg);
  font-size: 13px;
  line-height: 1.5;
  color: var(--vp-c-text-1);
}
.tsr-route-readout strong {
  color: var(--vp-c-text-1);
}
.tsr-link {
  cursor: pointer;
  transition: opacity 0.2s ease;
}
.tsr-link.is-dim {
  opacity: 0.28;
}
.tsr-link.is-dim:hover,
.tsr-link.is-dim:focus-visible {
  opacity: 0.85;
}
.tsr-link:hover rect,
.tsr-link:focus-visible rect {
  fill: var(--vp-c-brand-soft);
}
.tsr-link:focus {
  outline: none;
}
</style>
