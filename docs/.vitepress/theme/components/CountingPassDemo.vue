<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { computed } from 'vue'
import { bucketColor, recordLabel } from '../palette'
import { useStepper } from '../useStepper'
import StepControls from './StepControls.vue'

// One stable counting pass, as in `counting_pass` in engine.rs: histogram,
// exclusive prefix sum, then scatter into a second buffer.
const BUCKETS = 4
const keys = [31, 12, 7, 25, 14, 38, 3, 19, 22, 11, 30, 16]
const digit = (key: number) => Math.floor(key / 10)

type Phase = 'histogram' | 'prefix' | 'scatter' | 'done'
interface Frame {
  phase: Phase
  caption: string
  counts: number[]
  cursor: number[] | null
  output: (number | null)[]
  reading: number | null
  bucket: number | null
  wrote: number | null
}

const frames = computed<Frame[]>(() => {
  const out: Frame[] = []
  const counts = new Array(BUCKETS).fill(0)
  const output: (number | null)[] = new Array(keys.length).fill(null)
  const push = (f: Omit<Frame, 'counts' | 'output'> & { cursor: number[] | null }) =>
    out.push({ ...f, counts: [...counts], output: [...output], cursor: f.cursor ? [...f.cursor] : null })
  push({
    phase: 'histogram',
    caption: 'The digit is the tens place, so there are four buckets. First, count how many keys fall in each bucket.',
    cursor: null,
    reading: null,
    bucket: null,
    wrote: null,
  })
  keys.forEach((key, i) => {
    counts[digit(key)]++
    push({
      phase: 'histogram',
      caption: `Key ${key} has digit ${digit(key)}: bucket ${digit(key)} now counts ${counts[digit(key)]}.`,
      cursor: null,
      reading: i,
      bucket: digit(key),
      wrote: null,
    })
  })
  const cursor = new Array(BUCKETS).fill(0)
  let sum = 0
  for (let b = 0; b < BUCKETS; b++) {
    cursor[b] = sum
    sum += counts[b]
    push({
      phase: 'prefix',
      caption: `Bucket ${b} starts at output position ${cursor[b]}: the number of keys in the buckets before it.`,
      cursor: cursor.map((c, i) => (i <= b ? c : NaN)),
      reading: null,
      bucket: b,
      wrote: null,
    })
  }
  keys.forEach((key, i) => {
    const b = digit(key)
    const slot = cursor[b]++
    output[slot] = i
    push({
      phase: 'scatter',
      caption: `Key ${key} goes to position ${slot}, and bucket ${b}'s cursor moves to ${cursor[b]}. Reading the input front to back keeps equal digits in input order.`,
      cursor,
      reading: i,
      bucket: b,
      wrote: slot,
    })
  })
  push({
    phase: 'done',
    caption: 'Every key is grouped by its digit, and within a group the input order is kept: the pass is stable. It needed an output buffer as large as the input.',
    cursor,
    reading: null,
    bucket: null,
    wrote: null,
  })
  return out
})

const stepper = useStepper(computed(() => frames.value.length))
const frame = computed(() => frames.value[stepper.index.value])
const marks = computed(() => {
  const labels: Record<Phase, string> = {
    histogram: '1 · Histogram',
    prefix: '2 · Prefix sum',
    scatter: '3 · Scatter',
    done: 'Done',
  }
  const seen = new Set<Phase>()
  return frames.value.flatMap((f, at) => (seen.has(f.phase) ? [] : (seen.add(f.phase), [{ at, label: labels[f.phase] }])))
})

const CW = 40
const LEFT = 92
const width = LEFT + keys.length * CW + 12
</script>

<template>
  <figure class="tsr-figure">
    <svg :viewBox="`0 0 ${width} 250`" :width="width" role="img" aria-label="Counting pass: input, bucket counters and output buffer">
      <text x="0" y="32" font-size="12">Input</text>
      <g v-for="(key, i) in keys" :key="`in${i}`" class="tsr-cell">
        <rect
          :x="LEFT + i * CW + 2"
          y="12"
          :width="CW - 4"
          height="36"
          rx="5"
          :fill="frame.output.includes(i) ? 'var(--tsr-stale)' : bucketColor(digit(key), BUCKETS)"
          :stroke="frame.reading === i ? 'var(--tsr-focus)' : 'none'"
          stroke-width="2.5"
        />
        <text :x="LEFT + i * CW + CW / 2" y="31" text-anchor="middle" font-size="13" font-weight="600" :style="{ fill: frame.output.includes(i) ? 'var(--vp-c-text-3)' : 'var(--tsr-ink)' }">{{ key }}</text>
        <text :x="LEFT + i * CW + CW / 2" y="43" text-anchor="middle" font-size="9" :style="{ fill: frame.output.includes(i) ? 'var(--vp-c-text-3)' : 'var(--tsr-ink)' }">{{ recordLabel(i) }}</text>
      </g>

      <text x="0" y="96" font-size="12">Buckets</text>
      <g v-for="b in BUCKETS" :key="`b${b}`">
        <rect
          :x="LEFT + (b - 1) * 120 + 2"
          y="72"
          width="112"
          height="44"
          rx="6"
          fill="none"
          :stroke="frame.bucket === b - 1 ? 'var(--vp-c-brand-1)' : 'var(--tsr-line)'"
          :stroke-width="frame.bucket === b - 1 ? 2.5 : 1"
        />
        <rect :x="LEFT + (b - 1) * 120 + 8" y="80" width="12" height="12" rx="3" :fill="bucketColor(b - 1, BUCKETS)" />
        <text :x="LEFT + (b - 1) * 120 + 26" y="90" font-size="11">digit {{ b - 1 }}</text>
        <text :x="LEFT + (b - 1) * 120 + 8" y="109" font-size="11">
          count {{ frame.counts[b - 1] }}<tspan v-if="frame.cursor && !Number.isNaN(frame.cursor[b - 1])"> · next {{ frame.cursor[b - 1] }}</tspan>
        </text>
      </g>

      <text x="0" y="172" font-size="12">Output</text>
      <text x="0" y="186" font-size="10" opacity="0.7">n slots</text>
      <g v-for="(id, slot) in frame.output" :key="`out${slot}`" class="tsr-cell">
        <rect
          :x="LEFT + slot * CW + 2"
          y="152"
          :width="CW - 4"
          height="36"
          rx="5"
          :fill="id === null ? 'transparent' : bucketColor(digit(keys[id]), BUCKETS)"
          :stroke="frame.wrote === slot ? 'var(--vp-c-text-1)' : id === null ? 'var(--tsr-line)' : 'none'"
          :stroke-width="frame.wrote === slot ? 2.5 : 1"
          :stroke-dasharray="id === null ? '3 3' : undefined"
        />
        <template v-if="id !== null">
          <text :x="LEFT + slot * CW + CW / 2" y="171" text-anchor="middle" font-size="13" font-weight="600" style="fill: var(--tsr-ink)">{{ keys[id] }}</text>
          <text :x="LEFT + slot * CW + CW / 2" y="183" text-anchor="middle" font-size="9" style="fill: var(--tsr-ink)">{{ recordLabel(id) }}</text>
        </template>
        <text :x="LEFT + slot * CW + CW / 2" y="204" text-anchor="middle" font-size="9" opacity="0.6">{{ slot }}</text>
      </g>
      <text :x="LEFT" y="236" font-size="11" opacity="0.8">
        This n-record buffer is what Tessera avoids.
      </text>
    </svg>
    <p class="tsr-caption" aria-live="polite">{{ frame.caption }}</p>
    <StepControls
      :index="stepper.index.value"
      :last="stepper.last.value"
      :playing="stepper.playing.value"
      :speed="stepper.speed.value"
      :marks="marks"
      @toggle="stepper.toggle"
      @step="stepper.step"
      @seek="stepper.seek"
      @speed="(v) => (stepper.speed.value = v)"
    />
  </figure>
</template>
