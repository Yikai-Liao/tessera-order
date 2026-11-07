<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { computed, ref } from "vue";
import { bucketColor, recordLabel } from "../palette";
import {
  blockName,
  type Cell,
  type Frame,
  type Loc,
  type Phase,
  simulate,
} from "../sim/blockPartition";
import { useStepper } from "../useStepper";
import StepControls from "./StepControls.vue";

const BUCKETS = 4;
const classify = (key: number) => Math.floor(key / 10);

const blockSize = ref(3);
const length = ref(19);
const seed = ref(7);
const parallelPermutation = ref(false);

function makeKeys(n: number, s: number): number[] {
  let state = s * 2654435761;
  const next = () => {
    state = (state * 1103515245 + 12345) >>> 0;
    return state / 2 ** 32;
  };
  // Skewed bucket sizes make the bucket blocks fill at different rates.
  return Array.from({ length: n }, () => {
    const r = next();
    const bucket = r < 0.4 ? 1 : r < 0.6 ? 0 : r < 0.85 ? 3 : 2;
    return bucket * 10 + Math.floor(next() * 10);
  });
}

const keys = computed(() => makeKeys(length.value, seed.value));
const result = computed(() =>
  simulate({
    keys: keys.value,
    buckets: BUCKETS,
    blockSize: blockSize.value,
    classify,
    parallelPermutation: parallelPermutation.value,
  }),
);
const frames = computed(() => result.value.frames);
const stepper = useStepper(computed(() => frames.value.length));
const frame = computed<Frame>(
  () => frames.value[Math.min(stepper.index.value, frames.value.length - 1)],
);

const phaseLabels: Record<Phase, string> = {
  setup: "Set up blocks",
  scatter: "Scatter by digit",
  finish: "Rebuild block order",
  permute: "Parallel block permutation",
  compact: "Compact into input",
  done: "Done",
};
const marks = computed(() => {
  const seen = new Set<Phase>();
  const out: { at: number; label: string }[] = [];
  frames.value.forEach((f, at) => {
    if (!seen.has(f.phase)) {
      seen.add(f.phase);
      out.push({ at, label: phaseLabels[f.phase] });
    }
  });
  return out;
});

// Geometry
const CW = 24;
const CH = 34;
const GAP = 10;
const LEFT = 92;
const blockWidth = computed(() => blockSize.value * CW);
const blockStride = computed(() => blockWidth.value + GAP);
const ROW_SCRATCH_A = 30;
const ROW_SCRATCH_B = 110;
const ROW_INPUT = 200;
const ROW_ORDER = 300;

const scratchBlocks = computed(() => result.value.scratchBlocks);
const fullBlocks = computed(() => result.value.fullBlocks);
const tailLen = computed(() => length.value % blockSize.value);

interface BlockBox {
  physical: number;
  x: number;
  y: number;
}
const blocks = computed<BlockBox[]>(() => {
  const out: BlockBox[] = [];
  for (let p = 0; p < scratchBlocks.value; p++) {
    const row = p < BUCKETS ? ROW_SCRATCH_A : ROW_SCRATCH_B;
    out.push({ physical: p, x: LEFT + (p % BUCKETS) * blockStride.value, y: row });
  }
  for (let i = 0; i < fullBlocks.value; i++) {
    out.push({ physical: scratchBlocks.value + i, x: LEFT + i * blockStride.value, y: ROW_INPUT });
  }
  return out;
});
const tailX = computed(() => LEFT + fullBlocks.value * blockStride.value);

function cellPosition(loc: Loc): { x: number; y: number } {
  const b = blockSize.value;
  if (loc.region === "scratch") {
    const p = Math.floor(loc.index / b);
    const box = blocks.value[p];
    return { x: box.x + (loc.index % b) * CW, y: box.y };
  }
  const block = Math.floor(loc.index / b);
  return { x: LEFT + block * blockStride.value + (loc.index % b) * CW, y: ROW_INPUT };
}

const width = computed(() => {
  const inputRight = tailX.value + tailLen.value * CW;
  const scratchRight = LEFT + BUCKETS * blockStride.value;
  const orderRight = 16 + result.value.blockCount * 46;
  return Math.max(inputRight, scratchRight, orderRight) + 16;
});

const positions = computed(() => {
  const pos: number[] = [];
  for (const [logical, physical] of frame.value.order.entries()) pos[physical] = logical;
  return pos;
});

function role(physical: number): { text: string; color?: string } {
  const f = frame.value;
  const writer = f.writers?.findIndex((w) => w.physical === physical) ?? -1;
  if (writer >= 0) return { text: `→ bucket ${writer}`, color: bucketColor(writer, BUCKETS) };
  const logical = positions.value[physical];
  if (f.phase === "compact" || f.phase === "done") return { text: "" };
  if (logical < BUCKETS || logical >= f.activeEnd) return { text: "free" };
  return { text: "" };
}

const sameLoc = (a: Loc | null, b: Loc) => !!a && a.region === b.region && a.index === b.index;

interface DrawnCell {
  key: string;
  x: number;
  y: number;
  cell: Cell;
  read: boolean;
  wrote: boolean;
}
const drawnCells = computed<DrawnCell[]>(() => {
  const f = frame.value;
  const out: DrawnCell[] = [];
  const add = (region: "scratch" | "input", cells: Cell[]) =>
    cells.forEach((cell, index) => {
      const loc = { region, index };
      const { x, y } = cellPosition(loc);
      out.push({
        key: `${region}${index}`,
        x,
        y,
        cell,
        read: sameLoc(f.read, loc),
        wrote: f.wrote.some((w) => sameLoc(w, loc)),
      });
    });
  add("scratch", f.scratch);
  add("input", f.input);
  return out;
});

function cellFill(cell: Cell): string {
  if (cell.live && cell.rec) return bucketColor(cell.rec.bucket, BUCKETS);
  if (cell.rec) return "var(--tsr-stale)";
  return "transparent";
}

const orderChips = computed(() =>
  frame.value.order.map((physical, logical) => ({
    logical,
    physical,
    name: blockName(physical, scratchBlocks.value),
    bucket: frame.value.bucketIds[logical],
    active: logical >= BUCKETS && logical < frame.value.activeEnd,
  })),
);

function reshuffle() {
  seed.value = (seed.value * 31 + 17) % 9973;
}
</script>

<template>
  <figure class="tsr-figure">
    <div class="tsr-input">
      <label>
        Records
        <select v-model.number="length">
          <option :value="13">13</option>
          <option :value="19">19</option>
          <option :value="25">25</option>
        </select>
      </label>
      <label>
        Block size b
        <select v-model.number="blockSize">
          <option :value="2">2</option>
          <option :value="3">3</option>
          <option :value="4">4</option>
        </select>
      </label>
      <label>
        <input v-model="parallelPermutation" type="checkbox" />
        Parallel block permutation (Rayon)
      </label>
      <button type="button" class="tsr-controls-button" @click="reshuffle">New input</button>
      <span class="tsr-note">Digit = tens place, R = 4 buckets. Letters give each record's input position.</span>
    </div>

    <svg
      :viewBox="`0 0 ${width} ${ROW_ORDER + 64}`"
      :width="width"
      role="img"
      aria-label="Block partition: scratch blocks, input blocks and the logical block order"
    >
      <text x="0" :y="ROW_SCRATCH_A + 22" font-size="12">Scratch</text>
      <text x="0" :y="ROW_SCRATCH_A + 38" font-size="10" opacity="0.7">S0–S{{ BUCKETS - 1 }}</text>
      <text x="0" :y="ROW_SCRATCH_B + 22" font-size="12">Scratch</text>
      <text x="0" :y="ROW_SCRATCH_B + 38" font-size="10" opacity="0.7">S{{ BUCKETS }}–S{{ 2 * BUCKETS - 1 }}</text>
      <text x="0" :y="ROW_INPUT + 22" font-size="12">Input</text>
      <text x="0" :y="ROW_INPUT + 38" font-size="10" opacity="0.7">the slice</text>

      <!-- Physical blocks -->
      <g v-for="box in blocks" :key="box.physical">
        <text :x="box.x" :y="box.y - 6" font-size="11" opacity="0.8">
          {{ blockName(box.physical, scratchBlocks) }}
          <tspan v-if="frame.phase !== 'compact' && frame.phase !== 'done'" opacity="0.7" font-size="10">
            · L{{ positions[box.physical] }}
          </tspan>
        </text>
        <rect
          :x="box.x - 2"
          :y="box.y - 2"
          :width="blockWidth + 4"
          :height="CH + 4"
          rx="6"
          fill="none"
          :stroke="frame.focus.includes(box.physical) ? 'var(--vp-c-brand-1)' : 'var(--tsr-line)'"
          :stroke-width="frame.focus.includes(box.physical) ? 2.5 : 1"
        />
        <text
          v-if="role(box.physical).text"
          :x="box.x"
          :y="box.y + CH + 16"
          font-size="10"
          :style="{ fill: role(box.physical).color ? 'var(--vp-c-text-1)' : 'var(--vp-c-text-3)' }"
        >
          <tspan v-if="role(box.physical).color" :fill="role(box.physical).color">■ </tspan>{{ role(box.physical).text }}
        </text>
      </g>
      <!-- The input tail is not a physical block: it starts in scratch. -->
      <rect
        v-if="tailLen"
        :x="tailX - 2"
        :y="ROW_INPUT - 2"
        :width="tailLen * CW + 4"
        :height="CH + 4"
        rx="6"
        fill="none"
        stroke="var(--tsr-line)"
        stroke-dasharray="4 3"
      />
      <text v-if="tailLen" :x="tailX" :y="ROW_INPUT - 6" font-size="11" opacity="0.8">tail</text>

      <!-- Cells -->
      <g v-for="c in drawnCells" :key="c.key" class="tsr-cell">
        <rect
          :x="c.x + 1"
          :y="c.y + 1"
          :width="CW - 2"
          :height="CH - 2"
          rx="4"
          :fill="cellFill(c.cell)"
          :stroke="c.read ? 'var(--tsr-focus)' : c.wrote ? 'var(--vp-c-text-1)' : c.cell.rec ? 'none' : 'var(--tsr-line)'"
          :stroke-width="c.read || c.wrote ? 2.5 : 1"
          :stroke-dasharray="c.cell.rec || c.read || c.wrote ? undefined : '2 2'"
        />
        <template v-if="c.cell.rec">
          <text
            :x="c.x + CW / 2"
            :y="c.y + 16"
            text-anchor="middle"
            font-size="12"
            font-weight="600"
            :style="{ fill: c.cell.live ? 'var(--tsr-ink)' : 'var(--vp-c-text-3)' }"
            :opacity="c.cell.live ? 1 : 0.6"
          >
            {{ c.cell.rec.key }}
          </text>
          <text
            :x="c.x + CW / 2"
            :y="c.y + 28"
            text-anchor="middle"
            font-size="9"
            :style="{ fill: c.cell.live ? 'var(--tsr-ink)' : 'var(--vp-c-text-3)' }"
            :opacity="c.cell.live ? 0.75 : 0.5"
          >
            {{ recordLabel(c.cell.rec.id) }}
          </text>
        </template>
      </g>

      <!-- Logical block order -->
      <text x="0" :y="ROW_ORDER - 14" font-size="12">Logical block order (the block table)</text>
      <g v-for="chip in orderChips" :key="chip.logical">
        <rect
          :x="16 + chip.logical * 46"
          :y="ROW_ORDER"
          width="42"
          height="26"
          rx="5"
          :fill="chip.bucket !== null && chip.bucket !== undefined ? bucketColor(chip.bucket, BUCKETS) : 'transparent'"
          :stroke="chip.active ? 'var(--vp-c-text-2)' : 'var(--tsr-line)'"
          :stroke-dasharray="chip.active ? undefined : '3 3'"
        />
        <text
          :x="16 + chip.logical * 46 + 21"
          :y="ROW_ORDER + 17"
          text-anchor="middle"
          font-size="11"
          :style="{ fill: chip.bucket !== null && chip.bucket !== undefined ? 'var(--tsr-ink)' : 'var(--vp-c-text-1)' }"
        >
          {{ chip.name }}
        </text>
        <text :x="16 + chip.logical * 46 + 21" :y="ROW_ORDER + 40" text-anchor="middle" font-size="9" opacity="0.6">
          L{{ chip.logical }}
        </text>
      </g>
      <text x="16" :y="ROW_ORDER + 58" font-size="10" opacity="0.7">
        Solid outline: active blocks L{{ BUCKETS }}–L{{ frame.activeEnd - 1 }}. Dashed: free blocks. Fill: the bucket that filled the block.
      </text>
    </svg>

    <p class="tsr-caption" aria-live="polite"><strong>{{ phaseLabels[frame.phase] }}.</strong> {{ frame.caption }}</p>
    <div class="tsr-legend">
      <span v-for="b in BUCKETS" :key="b">
        <span class="swatch" :style="{ background: bucketColor(b - 1, BUCKETS) }"></span>bucket {{ b - 1 }} (keys {{ (b - 1) * 10 }}–{{ (b - 1) * 10 + 9 }})
      </span>
      <span><span class="swatch" style="background: var(--tsr-stale)"></span>stale bytes (moved away)</span>
      <span><span class="swatch" style="border: 2px solid var(--tsr-focus)"></span>being read</span>
    </div>
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

<style scoped>
.tsr-controls-button {
  padding: 3px 9px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 6px;
  background: var(--vp-c-bg);
  font-size: 13px;
}
</style>
