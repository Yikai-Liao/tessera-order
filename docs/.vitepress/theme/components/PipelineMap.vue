<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { useRouter, withBase } from "vitepress";

// The route an input takes through `engine.rs`. Every box links to the page
// that explains it. Boxes navigate with the router instead of SVG `<a>`
// elements, whose `href` VitePress's link prefetcher cannot read.
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
const boxes: Box[] = [
  {
    id: "probe",
    x: 200,
    y: 10,
    w: 280,
    title: "Order probe",
    detail: "unit stride: adjacent scan; otherwise sample",
    link: "/explain/routing#order-probe",
  },
  {
    id: "sorted",
    x: 530,
    y: 10,
    w: 160,
    title: "Done",
    detail: "or reverse stably",
    link: "/explain/routing#order-probe",
    kind: "exit",
  },
  {
    id: "insertion",
    x: 200,
    y: 80,
    w: 280,
    title: "Insertion",
    detail: "finish small tiles · budget larger inputs",
    link: "/explain/routing#budgeted-insertion",
  },
  {
    id: "varying",
    x: 200,
    y: 150,
    w: 280,
    title: "Varying bits",
    detail: "drop bits equal in every key",
    link: "/explain/keys#varying-bits",
  },
  {
    id: "constant",
    x: 530,
    y: 150,
    w: 160,
    title: "Done",
    detail: "all keys equal",
    link: "/explain/keys#varying-bits",
    kind: "exit",
  },
  {
    id: "node",
    x: 200,
    y: 230,
    w: 280,
    title: "Larger than a tile?",
    detail: "64 KiB of records",
    link: "/explain/routing#nodes-and-tiles",
  },
  {
    id: "mode",
    x: 30,
    y: 320,
    w: 190,
    title: "Repeated key",
    detail: "equality partition",
    link: "/explain/routing#repeated-key",
    kind: "node",
  },
  {
    id: "outlier",
    x: 260,
    y: 320,
    w: 190,
    title: "Outlier run",
    detail: "lift a short end run",
    link: "/explain/routing#outlier-run",
    kind: "node",
  },
  {
    id: "digit",
    x: 490,
    y: 320,
    w: 190,
    title: "Top digit",
    detail: "stable block partition",
    link: "/explain/block-partition",
    kind: "node",
  },
  {
    id: "leafI",
    x: 30,
    y: 456,
    w: 190,
    title: "Insertion sort",
    detail: "≤ 48 records",
    link: "/explain/tiles#insertion",
    kind: "tile",
  },
  {
    id: "leafM",
    x: 260,
    y: 456,
    w: 190,
    title: "Tile MSD",
    detail: "partition, then recurse",
    link: "/explain/tiles#tile-msd",
    kind: "tile",
  },
  {
    id: "leafL",
    x: 490,
    y: 456,
    w: 190,
    title: "Tile LSD",
    detail: "counting passes in scratch",
    link: "/explain/tiles#tile-lsd",
    kind: "tile",
  },
];
const H = 46;
const centers = [125, 355, 585];
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
  <figure class="tsr-figure">
    <svg viewBox="0 0 700 516" width="700" role="img" aria-label="Tessera's pipeline from order probe to tile leaves">
      <defs>
        <marker id="tsr-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
          <path d="M0,0 L10,5 L0,10 z" fill="var(--vp-c-text-3)" />
        </marker>
        <marker id="tsr-arrow-node" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
          <path d="M0,0 L10,5 L0,10 z" fill="#5CA4F8" />
        </marker>
      </defs>

      <g stroke="var(--vp-c-text-3)" stroke-width="1.5" fill="none">
        <path d="M480,33 H526" marker-end="url(#tsr-arrow)" />
        <path d="M340,56 V76" marker-end="url(#tsr-arrow)" />
        <path d="M480,103 C505,103 505,62 526,48" marker-end="url(#tsr-arrow)" />
        <path d="M340,126 V146" marker-end="url(#tsr-arrow)" />
        <path d="M480,173 H526" marker-end="url(#tsr-arrow)" />
        <path d="M340,196 V226" marker-end="url(#tsr-arrow)" />
        <!-- yes: node partitions -->
        <path d="M300,276 C300,298 125,294 125,316" marker-end="url(#tsr-arrow)" />
        <path d="M340,276 C340,298 355,294 355,316" marker-end="url(#tsr-arrow)" />
        <path d="M380,276 C380,298 585,294 585,316" marker-end="url(#tsr-arrow)" />
        <!-- no: tile leaves -->
        <path d="M480,253 H690 V428 H125" />
        <path v-for="x in centers" :key="`leaf${x}`" :d="`M${x},428 V452`" marker-end="url(#tsr-arrow)" />
      </g>

      <!-- Children re-enter the node test. -->
      <g stroke="#5CA4F8" stroke-width="1.5" stroke-dasharray="5 4" fill="none">
        <path v-for="x in centers" :key="`child${x}`" :d="`M${x},366 V392`" />
        <path d="M585,392 H12 V253 H196" marker-end="url(#tsr-arrow-node)" />
      </g>

      <g font-size="10" style="fill: var(--vp-c-text-2)">
        <text x="486" y="26">ordered</text>
        <text x="488" y="84">sorted</text>
        <text x="486" y="166">none</text>
        <text x="520" y="246">no: a tile</text>
        <text x="490" y="272">yes: a node</text>
        <text x="236" y="408">each child is routed again</text>
        <text x="140" y="422">the cost model picks one leaf</text>
      </g>

      <!-- biome-ignore lint/a11y/useSemanticElements: SVG `<a>` breaks VitePress prefetching (see the script comment). -->
      <g
        v-for="b in boxes"
        :key="b.id"
        class="tsr-link"
        role="link"
        tabindex="0"
        :aria-label="`${b.title}: ${b.detail}`"
        @click="go(b.link)"
        @keydown.enter="go(b.link)"
      >
        <rect :x="b.x" :y="b.y" :width="b.w" :height="H" rx="8" fill="var(--vp-c-bg)" :stroke="stroke(b)" stroke-width="2" />
        <text :x="b.x + b.w / 2" :y="b.y + 20" text-anchor="middle" font-size="13" font-weight="600">{{ b.title }}</text>
        <text :x="b.x + b.w / 2" :y="b.y + 36" text-anchor="middle" font-size="10" style="fill: var(--vp-c-text-2)">{{ b.detail }}</text>
      </g>
    </svg>
    <div class="tsr-legend">
      <span><span class="swatch" style="border: 2px solid #1baf7a"></span>early exit</span>
      <span><span class="swatch" style="border: 2px solid #5CA4F8"></span>node partitions (whole input, O(√n) scratch)</span>
      <span><span class="swatch" style="border: 2px solid #B482F8"></span>tile leaves (at most 64 KiB of scratch)</span>
      <span>Select a box to read about it.</span>
    </div>
  </figure>
</template>

<style scoped>
.tsr-link {
  cursor: pointer;
}
.tsr-link:hover rect,
.tsr-link:focus-visible rect {
  fill: var(--vp-c-brand-soft);
}
.tsr-link:focus {
  outline: none;
}
</style>
