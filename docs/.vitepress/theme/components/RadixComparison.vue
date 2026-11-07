<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { computed, ref } from 'vue'
import { comparisonInput } from '../sim/comparisonInput'
import { blockLsd, instabilities, lsd, msdInPlace, tessera, type Stage } from '../sim/radixVariants'
import { bucketColor, recordLabel } from '../palette'

interface Lane {
  name: string
  stable: boolean
  memory: string
  order: string
  stages: Stage[]
}

const lanes = computed<Lane[]>(() => [
  { name: 'LSD radix sort', stable: true, memory: 'n records', order: 'low digit → high digit', stages: lsd(comparisonInput) },
  { name: 'Block LSD', stable: true, memory: 'O(√n)', order: 'low digit → high digit', stages: blockLsd(comparisonInput) },
  { name: 'In-place MSD', stable: false, memory: 'O(1) per level', order: 'high digit → low digit', stages: msdInPlace(comparisonInput) },
  { name: 'Tessera', stable: true, memory: 'O(√n)', order: 'high digit split, then tiles', stages: tessera(comparisonInput) },
])

const stage = ref(2)
const tens = (key: number) => Math.floor(key / 10)

function groupStarts(s: Stage): Set<number> {
  return new Set((s.groups ?? []).map(([start]) => start).filter((start) => start > 0))
}
</script>

<template>
  <figure class="tsr-figure">
    <div class="tsr-input">
      <span>Show</span>
      <label v-for="(label, i) in ['input', 'after the first pass or level', 'the result']" :key="i">
        <input v-model.number="stage" type="radio" name="radix-stage" :value="i" />
        {{ label }}
      </label>
    </div>
    <div class="lanes">
      <section v-for="lane in lanes" :key="lane.name" class="lane">
        <header>
          <strong>{{ lane.name }}</strong>
          <span class="badge" :class="lane.stable ? 'good' : 'bad'">{{ lane.stable ? '✓ stable' : '✗ not stable' }}</span>
          <span class="badge">extra memory: {{ lane.memory }}</span>
          <span class="badge">{{ lane.order }}</span>
        </header>
        <div class="stage-label">{{ lane.stages[stage].label }}<span v-if="lane.stages[stage].extra"> · uses {{ lane.stages[stage].extra }}</span></div>
        <div class="row">
          <template v-for="(item, i) in lane.stages[stage].items" :key="item.id">
            <span v-if="groupStarts(lane.stages[stage]).has(i)" class="split" aria-hidden="true"></span>
            <span
              class="cell"
              :class="{ unstable: stage === 2 && instabilities(lane.stages[stage].items).has(i) }"
              :style="{ background: bucketColor(tens(item.key) - 1, 3) }"
            >
              <b>{{ item.key }}</b><small>{{ recordLabel(item.id) }}</small>
            </span>
          </template>
        </div>
      </section>
    </div>
    <p class="tsr-note">
      Colour is the tens digit; the letter is the input position. In a stable result, equal keys keep their letters in
      alphabetical order. A red outline marks equal keys that the in-place MSD sort swapped past each other.
    </p>
  </figure>
</template>

<style scoped>
.lanes {
  display: grid;
  gap: 14px;
}
.lane header {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px 10px;
  font-size: 14px;
}
.badge {
  padding: 0 8px;
  border: 1px solid var(--tsr-line);
  border-radius: 999px;
  font-size: 12px;
  color: var(--vp-c-text-2);
}
.badge.good {
  border-color: #1baf7a;
  color: var(--vp-c-text-1);
}
.badge.bad {
  border-color: var(--tsr-focus);
  color: var(--vp-c-text-1);
}
.stage-label {
  margin: 4px 0;
  font-size: 12px;
  color: var(--vp-c-text-2);
}
.row {
  display: flex;
  flex-wrap: wrap;
  gap: 3px;
  align-items: stretch;
}
.cell {
  display: inline-flex;
  flex-direction: column;
  align-items: center;
  width: 34px;
  padding: 2px 0;
  border-radius: 5px;
  border: 2px solid transparent;
  color: var(--tsr-ink);
  font-family: var(--vp-font-family-mono);
  line-height: 1.15;
  transition: background 0.25s ease;
}
.cell b {
  font-size: 13px;
}
.cell small {
  font-size: 10px;
  opacity: 0.75;
}
.cell.unstable {
  border-color: var(--tsr-focus);
}
.split {
  width: 2px;
  margin: 0 4px;
  background: var(--vp-c-text-3);
  border-radius: 1px;
}
</style>
