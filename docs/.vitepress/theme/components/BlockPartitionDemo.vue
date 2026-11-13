<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { computed, onMounted, ref, shallowRef } from "vue";
import { bucketColor } from "../palette";
import {
  blockName,
  type Cell,
  type Frame,
  type Loc,
  type Phase,
  type Prefix,
  simulate,
} from "../sim/blockPartition";
import { type Engine, instantiate } from "../sim/engine";
import wasmUrl from "../sim/tessera_trace.wasm?url";
import { useStepper } from "../useStepper";

// Two-digit keys 0–39 split on their tens digit: 2^RADIX = 4 buckets.
const RADIX = 2;
const BUCKETS = 1 << RADIX;
const DIVISOR = 10;

// The figure replays the engine's own partition, compiled to WebAssembly.
const engine = shallowRef<Engine | null>(null);
const failed = ref(false);
onMounted(async () => {
  try {
    engine.value = await instantiate(await (await fetch(wasmUrl)).arrayBuffer());
  } catch (error) {
    failed.value = true;
    console.error(error);
  }
});

const blockSize = ref(3);
const length = ref(19);
const seed = ref(7);

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
  engine.value
    ? simulate(engine.value, {
        keys: keys.value,
        radix: RADIX,
        blockSize: blockSize.value,
        divisor: DIVISOR,
      })
    : null,
);
// Records of a new input must not fly from the old input's places.
const run = computed(() => `${seed.value}-${length.value}-${blockSize.value}`);
const frames = computed(() => result.value?.frames ?? []);
const stepper = useStepper(computed(() => frames.value.length));
// Read only while the template shows the engine's result.
const frame = computed<Frame>(
  () => frames.value[Math.min(stepper.index.value, frames.value.length - 1)],
);

// Navigation follows the algorithm: phases, input positions and ledger sources.
const phases: { label: string; phases: Phase[] }[] = [
  { label: "Scatter", phases: ["setup", "scatter"] },
  { label: "Ledger", phases: ["ledger"] },
  { label: "Complete", phases: ["complete", "done"] },
];
const phaseStart = (index: number) =>
  frames.value.findIndex((f) => phases[index].phases.includes(f.phase));
const activePhase = computed(() => phases.findIndex((p) => p.phases.includes(frame.value.phase)));
const readFrame = computed(() => {
  const at = new Map<number, number>();
  frames.value.forEach((f, i) => {
    if (f.reads !== undefined) at.set(f.reads, i);
  });
  return at;
});
const sourceFrame = computed(() => {
  const at = new Map<number, number>();
  frames.value.forEach((f, i) => {
    if (f.source !== undefined) at.set(f.source, i);
  });
  return at;
});
function onKey(event: KeyboardEvent) {
  if (event.key === "ArrowRight") stepper.step(1);
  else if (event.key === "ArrowLeft") stepper.step(-1);
  else if (event.key === " ") stepper.toggle();
  else return;
  event.preventDefault();
}

// Geometry. The input row, the scratch rows, then one row per bucket.
const CW = 26;
const CH = 34;
const GAP = 12;
const ARROW = 22;
const LEFT = 86;
const Y_INDEX = 30;
const Y_INPUT = 36;
const Y_SCRATCH = [136, 206];
const Y_BUCKETS = 316;
const BUCKET_ROW = 64;

const b = computed(() => blockSize.value);
const stride = computed(() => b.value * CW + GAP);
const scratchBlocks = computed(() => result.value?.scratchBlocks ?? 0);
const fullBlocks = computed(() => result.value?.fullBlocks ?? 0);

/** Top-left corner of a physical block. */
function blockXY(physical: number) {
  if (physical < scratchBlocks.value) {
    return {
      x: LEFT + (physical % BUCKETS) * stride.value,
      y: Y_SCRATCH[physical < BUCKETS ? 0 : 1],
    };
  }
  return { x: LEFT + (physical - scratchBlocks.value) * stride.value, y: Y_INPUT };
}
function inputXY(index: number) {
  return {
    x: LEFT + Math.floor(index / b.value) * stride.value + (index % b.value) * CW,
    y: Y_INPUT,
  };
}
function locXY(loc: Loc) {
  if (loc.region === "input") return inputXY(loc.index);
  const origin = blockXY(Math.floor(loc.index / b.value));
  return { x: origin.x + (loc.index % b.value) * CW, y: origin.y };
}
const locKey = (loc: Loc) => `${loc.region}${loc.index}`;

const physicalBlocks = computed(() =>
  Array.from({ length: scratchBlocks.value + fullBlocks.value }, (_, p) => ({
    physical: p,
    ...blockXY(p),
  })),
);
const inputPositions = computed(() =>
  Array.from({ length: length.value }, (_, i) => ({ index: i, ...inputXY(i) })),
);
const tailLen = computed(() => length.value % b.value);

interface Chip {
  key: string;
  x: number;
  y: number;
  cell: Cell;
  marked: boolean;
}
const cellsOf = (f: Frame) => [
  ...f.scratch.map((cell, index) => ({ cell, loc: { region: "scratch", index } as Loc })),
  ...f.input.map((cell, index) => ({ cell, loc: { region: "input", index } as Loc })),
];
const wrote = computed(() => new Set(frame.value.wrote.map(locKey)));
/** Live records, keyed by identity so that each one glides to its next cell. */
const liveChips = computed<Chip[]>(() =>
  cellsOf(frame.value)
    .filter(({ cell }) => cell.live && cell.rec)
    .map(({ cell, loc }) => ({
      key: `${run.value}-${cell.rec!.id}`,
      ...locXY(loc),
      cell,
      marked: wrote.value.has(locKey(loc)),
    })),
);
/** Stale copies stay where they were left. */
const staleChips = computed<Chip[]>(() =>
  cellsOf(frame.value)
    .filter(({ cell }) => !cell.live && cell.rec)
    .map(({ cell, loc }) => ({ key: locKey(loc), ...locXY(loc), cell, marked: false })),
);

/** Bucket rows: each bucket's chain of block prefixes, holding the same records as above. */
const chainStride = computed(() => b.value * CW + ARROW);
function prefixRecords(prefix: Prefix) {
  const origin = prefix.physical * b.value;
  return Array.from({ length: prefix.len }, (_, slot) => {
    const loc: Loc =
      prefix.physical < scratchBlocks.value
        ? { region: "scratch", index: origin + slot }
        : { region: "input", index: (prefix.physical - scratchBlocks.value) * b.value + slot };
    const cell = loc.region === "scratch" ? frame.value.scratch[loc.index] : frame.value.input[loc.index];
    return { slot, cell };
  });
}
const chainBoxes = computed(() =>
  frame.value.chains.flatMap((chain, bucket) =>
    chain.map((prefix, position) => ({
      key: prefix.source === undefined ? `p${prefix.physical}` : `s${prefix.source}`,
      bucket,
      position,
      prefix,
      x: LEFT + position * chainStride.value,
      y: Y_BUCKETS + bucket * BUCKET_ROW,
      records: prefixRecords(prefix),
      jump: prefix.source === undefined ? undefined : sourceFrame.value.get(prefix.source),
    })),
  ),
);
const longestChain = computed(() =>
  Math.max(1, ...frames.value.flatMap((f) => f.chains.map((chain) => chain.length))),
);

const bandsVisible = computed(() => frame.value.owners !== null || frame.value.phase === "done");
const bands = computed(() => result.value?.bands ?? []);

const width = computed(
  () =>
    Math.max(
      LEFT + (fullBlocks.value + 1) * stride.value,
      LEFT + BUCKETS * stride.value,
      LEFT + longestChain.value * chainStride.value,
    ) + 8,
);
const height = computed(() => Y_BUCKETS + BUCKETS * BUCKET_ROW - 10);

const hovered = ref<number | null>(null);
const emphasised = (physical: number) =>
  frame.value.focus.includes(physical) || hovered.value === physical;

function role(physical: number): { text: string; color?: string } {
  const f = frame.value;
  const writer = f.writers?.findIndex((w) => w.physical === physical) ?? -1;
  if (writer >= 0) return { text: `${writer} writes`, color: bucketColor(writer, BUCKETS) };
  const owner = f.owners?.[physical];
  if (owner === undefined) return { text: "" };
  if (typeof owner === "number") return { text: "source", color: bucketColor(owner, BUCKETS) };
  return { text: owner };
}

const tens = (key: number) => String(Math.floor(key / DIVISOR));
const ones = (key: number) => String(key % DIVISOR);

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
      <button type="button" class="tsr-button" @click="reshuffle">New input</button>
    </div>

    <p v-if="!result" class="tsr-caption">
      {{ failed ? "The figure engine failed to load." : "Loading the figure engine…" }}
    </p>
    <div v-else>
      <div class="tsr-bar">
        <ol class="tsr-phases">
          <li v-for="(p, i) in phases" :key="p.label">
            <button
              type="button"
              :class="{ active: i === activePhase, past: i < activePhase }"
              @click="stepper.seek(phaseStart(i))"
            >
              <span class="number">{{ i + 1 }}</span>{{ p.label }}
            </button>
          </li>
        </ol>
        <!-- biome-ignore lint/a11y/noNoninteractiveTabindex: the toolbar takes focus to receive the arrow-key and space shortcuts. -->
        <div class="tsr-steps" role="toolbar" tabindex="0" aria-label="Steps: arrow keys step, space plays" @keydown="onKey">
          <button type="button" class="tsr-button" aria-label="Previous step" title="Previous step (←)" @click="stepper.step(-1)">◀</button>
          <button type="button" class="tsr-button tsr-next" title="Next step (→)" @click="stepper.step(1)">Next ▶</button>
          <button type="button" class="tsr-button" :title="stepper.playing.value ? 'Pause (space)' : 'Play (space)'" @click="stepper.toggle">
            {{ stepper.playing.value ? "❚❚" : "Play" }}
          </button>
        </div>
      </div>
      <p class="tsr-caption" aria-live="polite">{{ frame.caption }}</p>

      <svg
        :viewBox="`0 0 ${width} ${height}`"
        :width="width"
        role="img"
        aria-label="The input slice, the scratch blocks, and each bucket's chain of blocks"
      >
        <!-- Row titles -->
        <text x="0" :y="Y_INPUT + 15" font-size="12" font-weight="600">Input</text>
        <text x="0" :y="Y_INPUT + 29" font-size="10" opacity="0.7">position ↑</text>
        <text x="0" :y="Y_SCRATCH[0] + 15" font-size="12" font-weight="600">Scratch</text>
        <text x="0" :y="Y_SCRATCH[0] + 29" font-size="10" opacity="0.7">first blocks</text>
        <text x="0" :y="Y_SCRATCH[1] + 15" font-size="12" font-weight="600">Scratch</text>
        <text x="0" :y="Y_SCRATCH[1] + 29" font-size="10" opacity="0.7">reserve</text>
        <text x="0" :y="Y_BUCKETS - 30" font-size="12" font-weight="600">
          Buckets: each one is a chain of blocks, read left to right
        </text>

        <!-- Input positions; during scatter, a click jumps to the step that reads it. -->
        <g v-for="pos in inputPositions" :key="`i${pos.index}`">
          <!-- biome-ignore lint/a11y/noStaticElementInteractions: pointer shortcuts; the step toolbar and its keys reach every step. -->
          <text
            :x="pos.x + CW / 2"
            :y="Y_INDEX"
            text-anchor="middle"
            font-size="10"
            :class="{ 'tsr-jump': readFrame.has(pos.index) }"
            :font-weight="frame.reads === pos.index ? 700 : 400"
            :style="{ fill: frame.reads === pos.index ? 'var(--tsr-focus)' : 'var(--vp-c-text-2)' }"
            @click="readFrame.has(pos.index) && stepper.seek(readFrame.get(pos.index)!)"
          >
            {{ pos.index }}
          </text>
          <rect
            :x="pos.x + 1"
            :y="Y_INPUT + 1"
            :width="CW - 2"
            :height="CH - 2"
            rx="4"
            fill="none"
            :stroke="frame.reads === pos.index ? 'var(--tsr-focus)' : 'var(--tsr-line)'"
            :stroke-width="frame.reads === pos.index ? 2.5 : 1"
            :stroke-dasharray="frame.reads === pos.index ? undefined : '2 2'"
          />
        </g>

        <!-- Physical blocks: outline, name and current role -->
        <!-- biome-ignore lint/a11y/noStaticElementInteractions: pointer shortcuts; the step toolbar and its keys reach every step. -->
        <g
          v-for="box in physicalBlocks"
          :key="`b${box.physical}`"
          @mouseenter="hovered = box.physical"
          @mouseleave="hovered = null"
        >
          <text :x="box.x" :y="box.y - (box.physical < scratchBlocks ? 6 : 20)" font-size="11" opacity="0.8">
            {{ blockName(box.physical, scratchBlocks) }}
          </text>
          <template v-if="box.physical < scratchBlocks">
            <rect
              v-for="slot in b"
              :key="slot"
              :x="box.x + (slot - 1) * CW + 1"
              :y="box.y + 1"
              :width="CW - 2"
              :height="CH - 2"
              rx="4"
              fill="none"
              stroke="var(--tsr-line)"
              stroke-dasharray="2 2"
            />
          </template>
          <rect
            :x="box.x - 2"
            :y="box.y - 2"
            :width="b * CW + 4"
            :height="CH + 4"
            rx="6"
            fill="none"
            :stroke="emphasised(box.physical) ? 'var(--vp-c-brand-1)' : 'var(--tsr-line)'"
            :stroke-width="emphasised(box.physical) ? 2.5 : 1"
          />
          <text v-if="role(box.physical).text" :x="box.x" :y="box.y + CH + (box.physical < scratchBlocks ? 14 : 26)" font-size="10">
            <tspan v-if="role(box.physical).color" :fill="role(box.physical).color">■ </tspan>
            <tspan :style="{ fill: role(box.physical).color ? 'var(--vp-c-text-1)' : 'var(--vp-c-text-3)' }">{{ role(box.physical).text }}</tspan>
          </text>
        </g>
        <template v-if="tailLen">
          <text :x="inputXY(fullBlocks * b).x" :y="Y_INPUT - 20" font-size="11" opacity="0.8">tail</text>
          <rect
            :x="inputXY(fullBlocks * b).x - 2"
            :y="Y_INPUT - 2"
            :width="tailLen * CW + 4"
            :height="CH + 4"
            rx="6"
            fill="none"
            stroke="var(--tsr-line)"
            stroke-dasharray="4 3"
          />
        </template>

        <!-- Final bands: where each bucket ends up in the input -->
        <template v-if="bandsVisible">
          <rect
            v-for="(band, k) in bands"
            v-show="band.len > 0"
            :key="`band${k}`"
            :x="inputXY(band.start).x + 1"
            :y="Y_INPUT + CH + 5"
            :width="inputXY(band.start + band.len - 1).x + CW - inputXY(band.start).x - 2"
            height="5"
            rx="2"
            :fill="bucketColor(k, BUCKETS)"
          />
        </template>

        <!-- Stale copies, then live records -->
        <g v-for="c in staleChips" :key="c.key" :transform="`translate(${c.x}, ${c.y})`">
          <rect x="1" y="1" :width="CW - 2" :height="CH - 2" rx="4" fill="var(--tsr-stale)" />
          <text :x="CW / 2" y="16" text-anchor="middle" font-size="12" opacity="0.35">{{ c.cell.rec!.key }}</text>
        </g>
        <g
          v-for="c in liveChips"
          :key="c.key"
          class="tsr-move"
          :style="{ transform: `translate(${c.x}px, ${c.y}px)` }"
        >
          <rect
            x="1"
            y="1"
            :width="CW - 2"
            :height="CH - 2"
            rx="4"
            :fill="bucketColor(c.cell.rec!.bucket, BUCKETS)"
            :stroke="c.marked ? 'var(--vp-c-text-1)' : 'none'"
            :stroke-width="2"
          />
          <text :x="CW / 2" y="16" text-anchor="middle" font-size="13" style="fill: var(--tsr-ink)">
            <tspan font-weight="800">{{ tens(c.cell.rec!.key) }}</tspan><tspan opacity="0.55">{{ ones(c.cell.rec!.key) }}</tspan>
          </text>
          <text :x="CW / 2" y="28" text-anchor="middle" font-size="9" style="fill: var(--tsr-ink)" opacity="0.7">
            {{ c.cell.rec!.id }}
          </text>
        </g>

        <!-- Bucket rows: the logical view of the same records -->
        <g v-for="k in BUCKETS" :key="`row${k}`">
          <rect x="0" :y="Y_BUCKETS + (k - 1) * BUCKET_ROW + 4" width="10" height="10" rx="2" :fill="bucketColor(k - 1, BUCKETS)" />
          <text x="15" :y="Y_BUCKETS + (k - 1) * BUCKET_ROW + 13" font-size="12" font-weight="600">bucket {{ k - 1 }}</text>
          <text x="0" :y="Y_BUCKETS + (k - 1) * BUCKET_ROW + 28" font-size="10" opacity="0.7">tens digit {{ k - 1 }}</text>
          <text v-if="bandsVisible && bands[k - 1]?.len" x="0" :y="Y_BUCKETS + (k - 1) * BUCKET_ROW + 42" font-size="10" opacity="0.7">
            → {{ bands[k - 1].start }}–{{ bands[k - 1].start + bands[k - 1].len - 1 }}
          </text>
          <text
            v-if="!frame.chains[k - 1]?.length"
            :x="LEFT"
            :y="Y_BUCKETS + (k - 1) * BUCKET_ROW + 22"
            font-size="11"
            opacity="0.5"
          >
            {{ frame.phase === "setup" ? "—" : bandsVisible ? "complete" : "empty" }}
          </text>
        </g>
        <!-- biome-ignore lint/a11y/noStaticElementInteractions: pointer shortcuts; the step toolbar and its keys reach every step. -->
        <g
          v-for="box in chainBoxes"
          :key="`${run}-${box.key}`"
          class="tsr-move"
          :class="{ 'tsr-jump': box.jump !== undefined }"
          :style="{ transform: `translate(${box.x}px, ${box.y}px)` }"
          @mouseenter="hovered = box.prefix.physical"
          @mouseleave="hovered = null"
          @click="box.jump !== undefined && stepper.seek(box.jump)"
        >
          <text x="0" y="-5" font-size="10" opacity="0.8">{{ blockName(box.prefix.physical, scratchBlocks) }}</text>
          <text v-if="box.position > 0" :x="-ARROW + 4" :y="CH / 2 + 4" font-size="13" opacity="0.6">→</text>
          <rect
            v-for="slot in b"
            :key="slot"
            :x="(slot - 1) * CW + 1"
            y="1"
            :width="CW - 2"
            :height="CH - 2"
            rx="4"
            fill="none"
            stroke="var(--tsr-line)"
            stroke-dasharray="2 2"
          />
          <rect
            x="-2"
            y="-2"
            :width="b * CW + 4"
            :height="CH + 4"
            rx="6"
            fill="none"
            :stroke="emphasised(box.prefix.physical) ? 'var(--vp-c-brand-1)' : bucketColor(box.bucket, BUCKETS)"
            :stroke-width="emphasised(box.prefix.physical) ? 2.5 : 1.5"
          />
          <g v-for="r in box.records" :key="r.slot" :transform="`translate(${r.slot * CW}, 0)`">
            <template v-if="r.cell.live && r.cell.rec">
              <rect x="1" y="1" :width="CW - 2" :height="CH - 2" rx="4" :fill="bucketColor(box.bucket, BUCKETS)" />
              <text :x="CW / 2" y="16" text-anchor="middle" font-size="13" style="fill: var(--tsr-ink)">
                <tspan font-weight="800">{{ tens(r.cell.rec.key) }}</tspan><tspan opacity="0.55">{{ ones(r.cell.rec.key) }}</tspan>
              </text>
              <text :x="CW / 2" y="28" text-anchor="middle" font-size="9" style="fill: var(--tsr-ink)" opacity="0.7">
                {{ r.cell.rec.id }}
              </text>
            </template>
          </g>
        </g>
      </svg>

      <div class="tsr-legend">
        <span>
          <span class="tsr-sample" :style="{ background: bucketColor(3, BUCKETS) }"><b>3</b><span>7</span><small>4</small></span>key 37 from input position 4; its bold tens digit picks the bucket
        </span>
        <span><span class="swatch" style="background: var(--tsr-stale)"></span>stale copy, already moved</span>
        <span><span class="swatch" style="border: 2px solid var(--tsr-focus)"></span>position being read</span>
        <span>Click a position or a ledger block to jump there; ← → step.</span>
      </div>
    </div>
  </figure>
</template>

<style scoped>
.tsr-steps:focus-visible {
  outline: 2px solid var(--vp-c-brand-1);
  outline-offset: 4px;
  border-radius: 8px;
}

.tsr-button {
  padding: 3px 10px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 6px;
  background: var(--vp-c-bg);
  color: var(--vp-c-text-1);
  font-size: 13px;
}

.tsr-button:hover {
  border-color: var(--vp-c-brand-1);
}

.tsr-bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.tsr-phases {
  display: flex;
  gap: 4px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.tsr-phases button {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 3px 12px 3px 4px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 999px;
  background: var(--vp-c-bg);
  color: var(--vp-c-text-2);
  font-size: 13px;
}

.tsr-phases .number {
  display: inline-grid;
  place-items: center;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: var(--vp-c-default-soft);
  font-size: 12px;
  font-weight: 600;
}

.tsr-phases button.past {
  color: var(--vp-c-text-1);
}

.tsr-phases button.active {
  border-color: var(--vp-c-brand-1);
  color: var(--vp-c-brand-1);
  font-weight: 600;
}

.tsr-phases button.active .number {
  background: var(--vp-c-brand-1);
  color: var(--vp-c-white);
}

.tsr-steps {
  display: flex;
  gap: 6px;
}

.tsr-next {
  min-width: 76px;
  font-weight: 600;
}

.tsr-figure .tsr-caption {
  min-height: 4.5em;
  margin: 10px 0 12px;
}

.tsr-jump {
  cursor: pointer;
}

.tsr-sample {
  display: inline-grid;
  grid-template-columns: auto auto;
  justify-items: center;
  margin-right: 6px;
  padding: 1px 5px;
  border-radius: 4px;
  color: #0b0b0b;
  font-family: var(--vp-font-family-mono);
  font-size: 12px;
  line-height: 1.1;
  vertical-align: middle;
}

.tsr-sample span {
  opacity: 0.55;
}

.tsr-sample small {
  grid-column: 1 / 3;
  font-size: 9px;
  opacity: 0.7;
}

@media (prefers-reduced-motion: reduce) {
  .tsr-move {
    transition: none;
  }
}
</style>
