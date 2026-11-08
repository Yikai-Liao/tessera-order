<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { computed, ref } from "vue";

// `default_block_size` in block.rs: scratch 2·R·b·s bytes plus 10·n/b bytes
// of block tables, balanced at b* = ceil(sqrt(10n / (2Rs))), scaled by 4, then
// floored at max(1024 / s, 32) records.
const logN = ref(7);
const recordBytes = ref(8);
const radix = ref(8);

const n = computed(() => Math.round(10 ** logN.value));
const R = computed(() => 2 ** radix.value);
const scratch = (b: number) => 2 * R.value * b * recordBytes.value;
const tables = (b: number) => (10 * n.value) / b;
const total = (b: number) => scratch(b) + tables(b);
const balanced = computed(() =>
  Math.ceil(Math.sqrt((10 * n.value) / (2 * R.value * recordBytes.value))),
);
const floors = computed(() => Math.max(Math.floor(1024 / recordBytes.value), 32));
const chosen = computed(() => Math.max(1, 4 * balanced.value, floors.value));
const lsdBuffer = computed(() => n.value * recordBytes.value);

// Log-log axes: both cost terms are straight lines and the total is a V.
const W = 640;
const H = 300;
const M = { left: 64, right: 16, top: 16, bottom: 40 };
const bMin = 1;
const bMax = 1 << 14;
const yMin = computed(() => Math.min(total(balanced.value), scratch(bMin), tables(bMax)) / 4);
const yMax = computed(() => Math.max(scratch(bMax), tables(bMin)) * 2);
const x = (b: number) =>
  M.left +
  ((Math.log2(b) - Math.log2(bMin)) / (Math.log2(bMax) - Math.log2(bMin))) * (W - M.left - M.right);
const y = (v: number) =>
  H -
  M.bottom -
  ((Math.log2(v) - Math.log2(yMin.value)) / (Math.log2(yMax.value) - Math.log2(yMin.value))) *
    (H - M.top - M.bottom);
const samples = Array.from({ length: 141 }, (_, i) => 2 ** (i / 10));
const path = (f: (b: number) => number) =>
  samples
    .map((b) => [x(b), y(f(b))] as const)
    .filter(([, py]) => py >= M.top - 1 && py <= H - M.bottom + 1)
    .map(([px, py], i) => `${i ? "L" : "M"}${px.toFixed(1)},${py.toFixed(1)}`)
    .join("");

const series = computed(() => [
  { name: "scratch blocks 2Rbs", color: "#2a78d6", d: path(scratch), dash: "" },
  { name: "block tables 10n/b", color: "#eb6834", d: path(tables), dash: "" },
  { name: "total", color: "#1baf7a", d: path(total), dash: "" },
]);

function bytes(v: number): string {
  const units = ["B", "KiB", "MiB", "GiB"];
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v < 10 && !Number.isInteger(v) ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
}

const xTicks = [1, 4, 16, 64, 256, 1024, 4096, 16384];
// Ticks at powers of two, so that every label is a round KiB/MiB value.
const yTicks = computed(() => {
  const low = Math.ceil(Math.log2(yMin.value));
  const high = Math.floor(Math.log2(yMax.value));
  const step = Math.max(1, Math.ceil((high - low) / 6));
  const out: number[] = [];
  for (let e = low; e <= high; e++) if (e % step === 0) out.push(2 ** e);
  return out;
});
const tableRows = computed(() => {
  const rows = [Math.max(1, Math.round(balanced.value / 4)), balanced.value, balanced.value * 4];
  if (!rows.includes(chosen.value)) rows.push(chosen.value);
  return rows.sort((a, b) => a - b);
});

const hover = ref<number | null>(null);
function onMove(event: PointerEvent) {
  const svg = event.currentTarget as SVGSVGElement;
  const box = svg.getBoundingClientRect();
  const px = ((event.clientX - box.left) / box.width) * W;
  if (px < M.left || px > W - M.right) {
    hover.value = null;
    return;
  }
  const t = (px - M.left) / (W - M.left - M.right);
  hover.value = Math.max(
    1,
    Math.round(2 ** (Math.log2(bMin) + t * (Math.log2(bMax) - Math.log2(bMin)))),
  );
}
</script>

<template>
  <figure class="tsr-figure">
    <div class="tsr-input">
      <label>n = {{ n.toLocaleString('en-US') }}
        <input v-model.number="logN" type="range" min="4" max="9" step="0.25" aria-label="Number of records, log scale" />
      </label>
      <label>record size s
        <select v-model.number="recordBytes">
          <option v-for="s in [4, 8, 16, 32, 64]" :key="s" :value="s">{{ s }} bytes</option>
        </select>
      </label>
      <label>digit
        <select v-model.number="radix">
          <option v-for="r in [4, 6, 8, 10, 12]" :key="r" :value="r">{{ r }} bits (R = {{ 2 ** r }})</option>
        </select>
      </label>
    </div>
    <svg
      :viewBox="`0 0 ${W} ${H}`"
      :width="W"
      role="img"
      aria-label="Block partition memory as a function of the block size"
      @pointermove="onMove"
      @pointerleave="hover = null"
    >
      <g v-for="t in yTicks" :key="`y${t}`">
        <line :x1="M.left" :x2="W - M.right" :y1="y(t)" :y2="y(t)" stroke="var(--tsr-line)" stroke-width="1" />
        <text :x="M.left - 6" :y="y(t) + 4" text-anchor="end" font-size="10" style="fill: var(--vp-c-text-2)">{{ bytes(t) }}</text>
      </g>
      <g v-for="t in xTicks" :key="`x${t}`">
        <line :x1="x(t)" :x2="x(t)" :y1="H - M.bottom" :y2="H - M.bottom + 4" stroke="var(--vp-c-text-3)" />
        <text :x="x(t)" :y="H - M.bottom + 16" text-anchor="middle" font-size="10" style="fill: var(--vp-c-text-2)">{{ t }}</text>
      </g>
      <text :x="(W + M.left) / 2" :y="H - 6" text-anchor="middle" font-size="11" style="fill: var(--vp-c-text-2)">
        block size b, in records (log scale)
      </text>
      <line :x1="M.left" :x2="W - M.right" :y1="H - M.bottom" :y2="H - M.bottom" stroke="var(--vp-c-text-3)" />

      <path v-for="s in series" :key="s.name" :d="s.d" fill="none" :stroke="s.color" stroke-width="2" stroke-linejoin="round" />

      <line :x1="x(balanced)" :x2="x(balanced)" :y1="M.top" :y2="H - M.bottom" stroke="var(--vp-c-text-3)" stroke-dasharray="3 3" />
      <text :x="x(balanced) + 4" :y="M.top + 10" font-size="10" style="fill: var(--vp-c-text-2)">b* = {{ balanced }}</text>
      <circle :cx="x(chosen)" :cy="y(total(chosen))" r="5" fill="#1baf7a" stroke="var(--tsr-panel)" stroke-width="2" />
      <text :x="x(chosen) + 8" :y="y(total(chosen)) + 16" font-size="10" style="fill: var(--vp-c-text-1)">
        used: b = {{ chosen }}
      </text>

      <g v-if="hover !== null">
        <line :x1="x(hover)" :x2="x(hover)" :y1="M.top" :y2="H - M.bottom" stroke="var(--vp-c-text-2)" />
        <g :transform="`translate(${Math.min(x(hover) + 8, W - 190)}, ${M.top + 20})`">
          <rect width="180" height="64" rx="6" fill="var(--vp-c-bg)" stroke="var(--tsr-line)" />
          <text x="8" y="16" font-size="11" style="fill: var(--vp-c-text-1)" font-weight="600">b = {{ hover }}</text>
          <text x="8" y="31" font-size="10" style="fill: var(--vp-c-text-1)">scratch {{ bytes(scratch(hover)) }}</text>
          <text x="8" y="44" font-size="10" style="fill: var(--vp-c-text-1)">tables {{ bytes(tables(hover)) }}</text>
          <text x="8" y="57" font-size="10" style="fill: var(--vp-c-text-1)">total {{ bytes(total(hover)) }}</text>
        </g>
      </g>
    </svg>
    <div class="tsr-legend">
      <span v-for="s in series" :key="s.name"><span class="swatch" :style="{ background: s.color }"></span>{{ s.name }}</span>
    </div>
    <p class="tsr-caption">
      At b = {{ chosen }}, one block partition needs {{ bytes(total(chosen)) }} beyond the input
      ({{ bytes(scratch(chosen)) }} of scratch blocks and {{ bytes(tables(chosen)) }} of tables). An LSD pass would need
      a {{ bytes(lsdBuffer) }} buffer, {{ Math.round(lsdBuffer / total(chosen)).toLocaleString('en-US') }}× as much.
    </p>
    <table class="tsr-table">
      <thead><tr><th>b</th><th>scratch 2Rbs</th><th>tables 10n/b</th><th>total</th></tr></thead>
      <tbody>
        <tr v-for="b in tableRows" :key="b">
          <td>{{ b }}{{ b === chosen ? ' (used)' : b === balanced ? ' (b*)' : '' }}</td>
          <td>{{ bytes(scratch(b)) }}</td>
          <td>{{ bytes(tables(b)) }}</td>
          <td>{{ bytes(total(b)) }}</td>
        </tr>
      </tbody>
    </table>
  </figure>
</template>
