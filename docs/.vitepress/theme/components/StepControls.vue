<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
defineProps<{
  index: number
  last: number
  playing: boolean
  speed: number
  marks?: { at: number; label: string }[]
}>()
const emit = defineEmits<{
  toggle: []
  step: [delta: number]
  seek: [to: number]
  speed: [value: number]
}>()
function onKey(event: KeyboardEvent) {
  if (event.key === 'ArrowRight') emit('step', 1)
  else if (event.key === 'ArrowLeft') emit('step', -1)
  else if (event.key === ' ') emit('toggle')
  else return
  event.preventDefault()
}
</script>

<template>
  <div class="tsr-controls" tabindex="0" @keydown="onKey" aria-label="Animation controls: arrow keys step, space plays">
    <button type="button" @click="emit('seek', 0)" title="First step" aria-label="First step">⏮</button>
    <button type="button" @click="emit('step', -1)" title="Previous step (←)" aria-label="Previous step">◀</button>
    <button type="button" class="tsr-play" @click="emit('toggle')" :aria-label="playing ? 'Pause' : 'Play'">
      {{ playing ? '❚❚ Pause' : '▶ Play' }}
    </button>
    <button type="button" @click="emit('step', 1)" title="Next step (→)" aria-label="Next step">▶</button>
    <button type="button" @click="emit('seek', last)" title="Last step" aria-label="Last step">⏭</button>
    <input
      class="tsr-scrub"
      type="range"
      min="0"
      :max="last"
      :value="index"
      aria-label="Step"
      @input="emit('seek', Number(($event.target as HTMLInputElement).value))"
    />
    <span class="tsr-count">{{ index + 1 }} / {{ last + 1 }}</span>
    <select :value="speed" aria-label="Speed" @change="emit('speed', Number(($event.target as HTMLSelectElement).value))">
      <option :value="0.5">0.5×</option>
      <option :value="1">1×</option>
      <option :value="2">2×</option>
      <option :value="4">4×</option>
    </select>
    <div v-if="marks?.length" class="tsr-marks">
      <button
        v-for="mark in marks"
        :key="mark.at"
        type="button"
        class="tsr-mark"
        :class="{ active: index >= mark.at }"
        @click="emit('seek', mark.at)"
      >
        {{ mark.label }}
      </button>
    </div>
  </div>
</template>
