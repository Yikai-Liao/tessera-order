<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { computed, ref } from "vue";
import { bucketColor, recordLabel } from "../palette";
import { blockPartitionMemory, formatBytesBinary, TILE_BYTES } from "../sim/blockSize";
import { comparisonInput } from "../sim/comparisonInput";
import {
  blockLsd,
  instabilities,
  lsd,
  msdInPlace,
  type Stage,
  tessera,
} from "../sim/radixVariants";

interface Lane {
  name: string;
  stable: boolean;
  ram10M: string;
  order: string;
  stages: Stage[];
}

const modeled10M = blockPartitionMemory(10_000_000, 256, 8);
const lsdLabel = formatBytesBinary(modeled10M.lsdBytes);
const partitionLabel = formatBytesBinary(modeled10M.totalBytes);
const tileLabel = formatBytesBinary(TILE_BYTES);

const lanes = computed<Lane[]>(() => [
  {
    name: "LSD radix sort",
    stable: true,
    ram10M: `${lsdLabel} (O(n))`,
    order: "low → high",
    stages: lsd(comparisonInput),
  },
  {
    name: "Block LSD",
    stable: true,
    ram10M: "O(√n) blocks",
    order: "low → high",
    stages: blockLsd(comparisonInput),
  },
  {
    name: "In-place MSD",
    stable: false,
    ram10M: "~0 (unstable)",
    order: "high → low",
    stages: msdInPlace(comparisonInput),
  },
  {
    name: "Tessera",
    stable: true,
    ram10M: `~${partitionLabel} partition + ${tileLabel} tile`,
    order: "high split → cache tiles",
    stages: tessera(comparisonInput),
  },
]);

const REPEATED = [12, 25, 31] as const;
const stage = ref(2);
const spotlight = ref<number | null>(12);
const tens = (key: number) => Math.floor(key / 10);

function groupStarts(s: Stage): Set<number> {
  return new Set((s.groups ?? []).map(([start]) => start).filter((start) => start > 0));
}
</script>

<template>
  <figure class="tsr-figure">
    <div class="rc-bar">
      <div class="tsr-seg" role="tablist" aria-label="Comparison stage">
        <button
          v-for="(label, i) in ['Start', 'After pass 1', 'Sorted']"
          :key="label"
          type="button"
          role="tab"
          :aria-selected="stage === i"
          :class="{ active: stage === i }"
          @click="stage = i"
        >
          {{ label }}
        </button>
      </div>
      <fieldset class="rc-pairs" aria-label="Spotlight repeated key">
        <span class="rc-label">Track equal pair:</span>
        <button
          v-for="k in REPEATED"
          :key="k"
          type="button"
          class="rc-chip"
          :class="{ active: spotlight === k }"
          @click="spotlight = spotlight === k ? null : k"
        >
          {{ k }}
        </button>
      </fieldset>
    </div>

    <div class="lanes">
      <section v-for="lane in lanes" :key="lane.name" class="lane" :class="{ tessera: lane.name === 'Tessera' }">
        <header>
          <strong>{{ lane.name }}</strong>
          <span class="badge" :class="lane.stable ? 'good' : 'bad'">{{ lane.stable ? 'stable' : 'breaks stability' }}</span>
          <span class="badge">10M×8B extra: {{ lane.ram10M }}</span>
          <span class="badge muted">{{ lane.order }}</span>
        </header>
        <div class="stage-label">
          {{ lane.stages[stage].label }}<span v-if="lane.stages[stage].extra"> · {{ lane.stages[stage].extra }}</span>
        </div>
        <div class="row">
          <template v-for="(item, i) in lane.stages[stage].items" :key="item.id">
            <span v-if="groupStarts(lane.stages[stage]).has(i)" class="split" aria-hidden="true"></span>
            <!-- biome-ignore lint/a11y/noStaticElementInteractions: hover previews an equal-key pair; the toolbar buttons above provide keyboard control. -->
            <span
              class="cell"
              :class="{
                unstable: stage === 2 && instabilities(lane.stages[stage].items).has(i),
                dimmed: spotlight !== null && item.key !== spotlight,
                focused: spotlight !== null && item.key === spotlight,
              }"
              :style="{ background: bucketColor(tens(item.key) - 1, 3) }"
              @mouseenter="REPEATED.includes(item.key as 12 | 25 | 31) && (spotlight = item.key)"
            >
              <b>{{ item.key }}</b><small>{{ recordLabel(item.id) }}</small>
            </span>
          </template>
        </div>
      </section>
    </div>

    <p class="tsr-caption">
      Small numbers show each record's original input position (<code>0</code>…<code>11</code>). In-place MSD's swap
      cycles flip equal keys out of input order (outlined in red); Tessera matches LSD's exact stable order while
      cutting modeled 10M-record 8-byte extra memory from {{ lsdLabel }} to ~{{ partitionLabel }} of partition scratch
      plus a {{ tileLabel }} tile buffer.
    </p>
  </figure>
</template>

<style scoped>
.rc-bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 10px 16px;
  margin-bottom: 14px;
}
.rc-pairs {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 0;
  padding: 0;
  border: none;
  font-size: 12px;
}
.rc-label {
  color: var(--vp-c-text-2);
}
.rc-chip {
  padding: 2px 10px;
  border: 1px solid var(--tsr-line);
  border-radius: 999px;
  background: var(--vp-c-bg);
  color: var(--vp-c-text-2);
  font-family: var(--vp-font-family-mono);
  font-size: 12px;
  cursor: pointer;
}
.rc-chip.active {
  border-color: var(--vp-c-brand-1);
  background: var(--vp-c-brand-soft);
  color: var(--vp-c-text-1);
  font-weight: 600;
}
.lanes {
  display: grid;
  gap: 12px;
}
.lane {
  padding: 10px 12px;
  border: 1px solid var(--tsr-line);
  border-radius: 10px;
  background: var(--vp-c-bg);
}
.lane.tessera {
  border-color: var(--vp-c-brand-1);
}
.lane header {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px 8px;
  font-size: 13.5px;
}
.badge {
  padding: 1px 8px;
  border: 1px solid var(--tsr-line);
  border-radius: 999px;
  font-size: 11.5px;
  color: var(--vp-c-text-2);
}
.badge.good {
  border-color: #1baf7a;
  color: var(--vp-c-text-1);
}
.badge.bad {
  border-color: var(--tsr-focus);
  color: var(--tsr-focus);
}
.badge.muted {
  opacity: 0.8;
}
.stage-label {
  margin: 5px 0 8px;
  font-size: 12px;
  color: var(--vp-c-text-2);
}
.row {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  align-items: stretch;
}
.cell {
  display: inline-flex;
  flex-direction: column;
  align-items: center;
  width: 36px;
  padding: 3px 0;
  border-radius: 6px;
  border: 2px solid transparent;
  color: var(--tsr-ink);
  font-family: var(--vp-font-family-mono);
  line-height: 1.15;
  transition: opacity 0.2s ease, transform 0.2s ease, border-color 0.2s ease;
}
.cell b {
  font-size: 13px;
}
.cell small {
  font-size: 10px;
  opacity: 0.78;
}
.cell.dimmed {
  opacity: 0.28;
}
.cell.focused {
  transform: translateY(-2px);
  border-color: var(--vp-c-text-1);
}
.cell.unstable {
  opacity: 1;
  border-color: var(--tsr-focus);
}
.split {
  width: 2px;
  margin: 0 5px;
  background: var(--vp-c-text-3);
  border-radius: 1px;
}
</style>
