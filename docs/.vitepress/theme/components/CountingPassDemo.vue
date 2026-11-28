<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { computed, ref } from "vue";
import { bucketColor, recordLabel } from "../palette";

// One stable counting pass on the tens digit (4 buckets: 0–9, 10–19, 20–29, 30–39).
const BUCKETS = 4;
const RANGE = ["0–9", "10–19", "20–29", "30–39"] as const;
const KEYS = [31, 12, 7, 25, 14, 38, 3, 19, 22, 11, 30, 16];
const digit = (key: number) => Math.floor(key / 10);

type Stage = "count" | "offsets" | "scatter";
const stage = ref<Stage>("scatter");
const activeBucket = ref<number | null>(null);
const activeIndex = ref<number | null>(1);

const counts = KEYS.reduce((acc, key) => {
  acc[digit(key)]++;
  return acc;
}, new Array<number>(BUCKETS).fill(0));

const offsets = counts.reduce<number[]>((acc, count, i) => {
  acc.push(i === 0 ? 0 : acc[i - 1] + counts[i - 1]);
  return acc;
}, []);

const placements = (() => {
  const cursor = [...offsets];
  return KEYS.map((key, i) => {
    const b = digit(key);
    const out = cursor[b]++;
    return { id: i, key, bucket: b, out };
  });
})();

const outputSlots = (() => {
  const slots = new Array<(typeof placements)[number]>(KEYS.length);
  for (const item of placements) slots[item.out] = item;
  return slots;
})();

const focusedBucket = computed(() =>
  activeBucket.value !== null
    ? activeBucket.value
    : activeIndex.value !== null
      ? placements[activeIndex.value].bucket
      : null,
);

const isHighlighted = (item: (typeof placements)[number]) => {
  if (activeIndex.value !== null) return item.id === activeIndex.value;
  if (activeBucket.value !== null) return item.bucket === activeBucket.value;
  return true;
};

const caption = computed(() => {
  if (stage.value === "count") {
    if (activeBucket.value !== null) {
      const b = activeBucket.value;
      return `Bucket ${b} (${RANGE[b]}) has ${counts[b]} keys. Counting them takes one sequential read over the input.`;
    }
    return "Step 1 (Histogram): scan the input once and count how many keys fall into each tens digit (0–3). Hover any bucket or key to trace it.";
  }
  if (stage.value === "offsets") {
    if (activeBucket.value !== null) {
      const b = activeBucket.value;
      return `Bucket ${b} (${RANGE[b]}) starts at output slot ${offsets[b]} and spans slots ${offsets[b]}–${offsets[b] + counts[b] - 1}.`;
    }
    return "Step 2 (Prefix sum): sum earlier bucket counts to reserve each bucket's exact contiguous slice in the output buffer.";
  }
  if (activeIndex.value !== null) {
    const item = placements[activeIndex.value];
    const displaced = KEYS[item.out];
    const clobber =
      item.out > item.id
        ? ` Writing directly into input slot ${item.out} would overwrite unread key ${displaced} (pos ${recordLabel(item.out)}).`
        : item.out < item.id
          ? ` Input slot ${item.out} was already read, but forward moves in the same pass (such as key 12 from pos 1 to slot 2) still collide with unread slots.`
          : " This record lands on its own index, yet forward moves in the same pass still collide with unread slots.";
    return `Key ${item.key} (pos ${recordLabel(item.id)}, digit ${item.bucket}) moves to output slot ${item.out}.${clobber}`;
  }
  if (activeBucket.value !== null) {
    const b = activeBucket.value;
    const members = placements
      .filter((p) => p.bucket === b)
      .map((p) => `${p.key} (${recordLabel(p.id)})`)
      .join(" → ");
    return `Bucket ${b} (${RANGE[b]}) fills slots ${offsets[b]}–${offsets[b] + counts[b] - 1} in input order: ${members}.`;
  }
  return "Step 3 (Scatter): reading left-to-right fills every bucket in input order (stable), but requires a full n-record output buffer. Hover any key or bucket to trace its exact path.";
});

const CW = 46;
const LEFT = 78;
const WIDTH = LEFT + KEYS.length * CW + 12;
const IN_Y = 16;
const BKT_Y = 92;
const OUT_Y = 186;
const cellX = (index: number) => LEFT + index * CW + CW / 2;
const bucketX = (b: number) => LEFT + b * 138 + 64;

function ribbonPath(item: (typeof placements)[number]) {
  const x1 = cellX(item.id);
  const y1 = IN_Y + 38;
  const x2 = cellX(item.out);
  const y2 = OUT_Y;
  const midY = (y1 + y2) / 2;
  return `M ${x1} ${y1} C ${x1} ${midY}, ${x2} ${midY}, ${x2} ${y2}`;
}

function selectBucket(b: number | null) {
  activeBucket.value = b;
  if (b !== null) activeIndex.value = null;
}

function selectRecord(i: number | null) {
  activeIndex.value = i;
  if (i !== null) activeBucket.value = null;
}
</script>

<template>
  <figure class="tsr-figure">
    <div class="tsr-seg" role="tablist" aria-label="Counting pass stages">
      <button
        v-for="[id, label] in [
          ['count', '1. Count digits'],
          ['offsets', '2. Compute offsets'],
          ['scatter', '3. Scatter to buffer'],
        ] as const"
        :key="id"
        type="button"
        role="tab"
        :aria-selected="stage === id"
        :class="{ active: stage === id }"
        @click="stage = id"
      >
        {{ label }}
      </button>
    </div>

    <svg
      :viewBox="`0 0 ${WIDTH} 254`"
      :width="WIDTH"
      role="img"
      aria-label="Counting pass showing input array, bucket counts and offsets, and output buffer with flow paths"
      @mouseleave="activeIndex = null; activeBucket = null"
    >
      <!-- Flow ribbons (visible during scatter) -->
      <g v-if="stage === 'scatter'">
        <path
          v-for="item in placements"
          :key="`arc-${item.id}`"
          :d="ribbonPath(item)"
          fill="none"
          :stroke="bucketColor(item.bucket, BUCKETS)"
          :stroke-width="activeIndex === item.id ? 3 : isHighlighted(item) ? 1.8 : 1"
          :opacity="isHighlighted(item) ? (activeIndex === item.id ? 0.95 : 0.6) : 0.12"
          class="tsr-flow"
        />
      </g>

      <!-- Input row -->
      <text x="0" :y="IN_Y + 23" font-size="12" font-weight="600">Input</text>
      <!-- biome-ignore lint/a11y/useSemanticElements: SVG has no native button element; the group exposes button semantics. -->
      <g
        v-for="item in placements"
        :key="`in-${item.id}`"
        role="button"
        tabindex="0"
        :aria-label="`Trace input key ${item.key} at slot ${item.id}`"
        style="cursor: pointer"
        @mouseenter="selectRecord(item.id)"
        @click="selectRecord(activeIndex === item.id ? null : item.id)"
        @keydown.enter.space.prevent="selectRecord(activeIndex === item.id ? null : item.id)"
      >
        <rect
          :x="LEFT + item.id * CW + 3"
          :y="IN_Y"
          :width="CW - 6"
          height="38"
          rx="6"
          :fill="bucketColor(item.bucket, BUCKETS)"
          :opacity="isHighlighted(item) ? 1 : 0.25"
          :stroke="activeIndex === item.id ? 'var(--vp-c-text-1)' : 'none'"
          stroke-width="2"
          class="tsr-flow"
        />
        <text
          :x="cellX(item.id)"
          :y="IN_Y + 18"
          text-anchor="middle"
          font-size="13"
          font-weight="700"
          :opacity="isHighlighted(item) ? 1 : 0.35"
          style="fill: var(--tsr-ink)"
        >
          {{ item.key }}
        </text>
        <text
          :x="cellX(item.id)"
          :y="IN_Y + 31"
          text-anchor="middle"
          font-size="9.5"
          :opacity="isHighlighted(item) ? 0.8 : 0.3"
          style="fill: var(--tsr-ink)"
        >
          {{ recordLabel(item.id) }}
        </text>
      </g>

      <!-- Bucket cards -->
      <text x="0" :y="BKT_Y + 26" font-size="12" font-weight="600">Buckets</text>
      <!-- biome-ignore lint/a11y/useSemanticElements: SVG has no native button element; the group exposes button semantics. -->
      <g
        v-for="b in BUCKETS"
        :key="`bkt-${b - 1}`"
        role="button"
        tabindex="0"
        :aria-label="`Trace bucket ${b - 1} (${RANGE[b - 1]})`"
        style="cursor: pointer"
        @mouseenter="selectBucket(b - 1)"
        @click="selectBucket(activeBucket === b - 1 ? null : b - 1)"
        @keydown.enter.space.prevent="selectBucket(activeBucket === b - 1 ? null : b - 1)"
      >
        <rect
          :x="LEFT + (b - 1) * 138 + 3"
          :y="BKT_Y"
          width="128"
          height="46"
          rx="8"
          fill="var(--vp-c-bg)"
          :stroke="focusedBucket === b - 1 ? bucketColor(b - 1, BUCKETS) : 'var(--tsr-line)'"
          :stroke-width="focusedBucket === b - 1 ? 2.5 : 1"
          class="tsr-flow"
        />
        <rect
          :x="LEFT + (b - 1) * 138 + 11"
          :y="BKT_Y + 10"
          width="11"
          height="11"
          rx="3"
          :fill="bucketColor(b - 1, BUCKETS)"
        />
        <text :x="LEFT + (b - 1) * 138 + 28" :y="BKT_Y + 19" font-size="11.5" font-weight="600">
          {{ RANGE[b - 1] }}
        </text>
        <text :x="LEFT + (b - 1) * 138 + 11" :y="BKT_Y + 36" font-size="11" opacity="0.85">
          {{ stage === 'count' ? `count = ${counts[b - 1]}` : `slots ${offsets[b - 1]}–${offsets[b - 1] + counts[b - 1] - 1}` }}
        </text>
      </g>

      <!-- Output buffer -->
      <text x="0" :y="OUT_Y + 18" font-size="12" font-weight="600">Output</text>
      <text x="0" :y="OUT_Y + 32" font-size="10" opacity="0.65">n slots</text>

      <!-- Bucket range brackets in offsets & scatter stages -->
      <g v-if="stage !== 'count'">
        <rect
          v-for="b in BUCKETS"
          :key="`band-${b - 1}`"
          :x="LEFT + offsets[b - 1] * CW + 2"
          :y="OUT_Y - 5"
          :width="counts[b - 1] * CW - 4"
          height="48"
          rx="8"
          fill="none"
          :stroke="bucketColor(b - 1, BUCKETS)"
          :stroke-width="focusedBucket === b - 1 ? 2 : 1"
          :stroke-dasharray="stage === 'offsets' ? '4 3' : undefined"
          :opacity="focusedBucket === null || focusedBucket === b - 1 ? 0.85 : 0.2"
        />
      </g>

      <!-- biome-ignore lint/a11y/useSemanticElements: SVG has no native button element; the group exposes button semantics. -->
      <g
        v-for="(item, slot) in outputSlots"
        :key="`out-${slot}`"
        role="button"
        tabindex="0"
        :aria-label="`Trace output slot ${slot} holding key ${item.key}`"
        style="cursor: pointer"
        @mouseenter="selectRecord(item.id)"
        @click="selectRecord(activeIndex === item.id ? null : item.id)"
        @keydown.enter.space.prevent="selectRecord(activeIndex === item.id ? null : item.id)"
      >
        <rect
          :x="LEFT + slot * CW + 3"
          :y="OUT_Y"
          :width="CW - 6"
          height="38"
          rx="6"
          :fill="stage === 'scatter' ? bucketColor(item.bucket, BUCKETS) : 'transparent'"
          :opacity="stage === 'scatter' ? (isHighlighted(item) ? 1 : 0.22) : 1"
          :stroke="
            stage === 'scatter'
              ? activeIndex === item.id
                ? 'var(--vp-c-text-1)'
                : 'none'
              : 'var(--tsr-line)'
          "
          :stroke-width="activeIndex === item.id ? 2 : 1"
          :stroke-dasharray="stage === 'scatter' ? undefined : '3 3'"
          class="tsr-flow"
        />
        <template v-if="stage === 'scatter'">
          <text
            :x="cellX(slot)"
            :y="OUT_Y + 18"
            text-anchor="middle"
            font-size="13"
            font-weight="700"
            :opacity="isHighlighted(item) ? 1 : 0.3"
            style="fill: var(--tsr-ink)"
          >
            {{ item.key }}
          </text>
          <text
            :x="cellX(slot)"
            :y="OUT_Y + 31"
            text-anchor="middle"
            font-size="9.5"
            :opacity="isHighlighted(item) ? 0.8 : 0.25"
            style="fill: var(--tsr-ink)"
          >
            {{ recordLabel(item.id) }}
          </text>
        </template>
        <text :x="cellX(slot)" :y="OUT_Y + 56" text-anchor="middle" font-size="9.5" opacity="0.55">
          {{ slot }}
        </text>
      </g>
    </svg>

    <p class="tsr-caption" aria-live="polite">{{ caption }}</p>
  </figure>
</template>

<style scoped>
.tsr-flow {
  transition: opacity 0.2s ease, stroke-width 0.2s ease, fill 0.2s ease;
}
</style>
