<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch } from "vue";
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
// Play pace in milliseconds per step; the default leaves time to read each caption.
const stepMs = ref(1600);
watch(stepMs, (ms) => (stepper.speed.value = 700 / ms), { immediate: true });
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

// Geometry. The figure is laid out for the width it has, wrapping blocks onto
// further lines instead of scaling text down, and keeps one size for a whole
// run so that nothing moves between steps except the records.
const CW = 22;
const CH = 34;
const GAP = 10;
const ARROW = 20;
/** Room above a block for its name, and above an input cell for its position. */
const NAME = 16;
const INDEX = 14;
/** Room below a block for its role label, and below the input for band bars. */
const ROLE = 18;
const BAND = 8;
const CHAIN_LINE = NAME + CH + 10;
/** Width of the longest role label, such as "■ 0 writes". */
const ROLE_WIDTH = 60;

const figure = ref<HTMLElement | null>(null);
const available = ref(680);
let observer: ResizeObserver | undefined;
onMounted(() => {
  observer = new ResizeObserver(([entry]) => {
    available.value = Math.floor(entry.contentRect.width);
  });
  if (figure.value) observer.observe(figure.value);
});
onBeforeUnmount(() => observer?.disconnect());

const b = computed(() => blockSize.value);
const scratchBlocks = computed(() => result.value?.scratchBlocks ?? 0);
const fullBlocks = computed(() => result.value?.fullBlocks ?? 0);
const tailLen = computed(() => length.value % b.value);

const layout = computed(() => {
  const width = Math.max(260, available.value);
  // Narrow screens put row titles above their rows instead of beside them.
  const compact = width < 560;
  const left = compact ? 0 : 100;
  const title = compact ? 20 : 0;
  // Narrow blocks still leave room for their role label.
  const stride = Math.max(b.value * CW, ROLE_WIDTH) + GAP;
  const perLine = Math.max(1, Math.floor((width - left + GAP) / stride));
  let y = 0;

  const inputTitle = y;
  y += title;
  const inputLine = NAME + INDEX + CH + BAND + ROLE;
  const inputTop = y + NAME + INDEX;
  // The tail is narrower than a block and follows the last full block when it fits.
  const fullLines = Math.ceil(fullBlocks.value / perLine);
  const lastLineBlocks = fullBlocks.value - (fullLines - 1) * perLine;
  const tailWraps =
    fullLines === 0 || left + lastLineBlocks * stride + tailLen.value * CW > width;
  const inputLines = fullLines + (tailLen.value && tailWraps ? 1 : 0);
  y += Math.max(1, inputLines) * inputLine;

  const scratchPerLine = Math.min(BUCKETS, perLine);
  const scratchLine = NAME + CH + ROLE;
  const scratchTitle: number[] = [];
  const scratchTop: number[] = [];
  for (let group = 0; group < 2; group++) {
    scratchTitle.push(y);
    y += title;
    scratchTop.push(y + NAME);
    y += Math.ceil(BUCKETS / scratchPerLine) * scratchLine + 4;
  }

  const bucketsTitle = y + 16;
  y += 30;
  const chainStride = b.value * CW + ARROW;
  const chainPerLine = Math.max(1, Math.floor((width - left + ARROW) / chainStride));
  const label = compact ? 18 : 0;
  const rows = Array.from({ length: BUCKETS }, (_, k) => {
    // The longest chain this bucket reaches in the run fixes its row height.
    const longest = Math.max(1, ...frames.value.map((f) => f.chains[k]?.length ?? 0));
    const top = y;
    const height = Math.max(label + Math.ceil(longest / chainPerLine) * CHAIN_LINE, compact ? 0 : 52);
    y += height + 6;
    return { top, boxTop: top + label + NAME };
  });
  return {
    width,
    height: y,
    compact,
    left,
    stride,
    perLine,
    inputTitle,
    inputTop,
    inputLine,
    tailWraps,
    scratchPerLine,
    scratchLine,
    scratchTitle,
    scratchTop,
    bucketsTitle,
    chainStride,
    chainPerLine,
    rows,
  };
});

function inputBlockXY(block: number) {
  const l = layout.value;
  if (block > 0 && block === fullBlocks.value) {
    // The tail: after the last full block, or at the start of the next line.
    const last: { x: number; y: number } = inputBlockXY(block - 1);
    return l.tailWraps
      ? { x: l.left, y: last.y + l.inputLine }
      : { x: last.x + l.stride, y: last.y };
  }
  return {
    x: l.left + (block % l.perLine) * l.stride,
    y: l.inputTop + Math.floor(block / l.perLine) * l.inputLine,
  };
}
/** Top-left corner of a physical block. */
function blockXY(physical: number) {
  const l = layout.value;
  if (physical >= scratchBlocks.value) return inputBlockXY(physical - scratchBlocks.value);
  const index = physical % BUCKETS;
  return {
    x: l.left + (index % l.scratchPerLine) * l.stride,
    y: l.scratchTop[physical < BUCKETS ? 0 : 1] + Math.floor(index / l.scratchPerLine) * l.scratchLine,
  };
}
function inputXY(index: number) {
  const origin = inputBlockXY(Math.floor(index / b.value));
  return { x: origin.x + (index % b.value) * CW, y: origin.y };
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
const tailXY = computed(() => inputBlockXY(fullBlocks.value));

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
function prefixRecords(prefix: Prefix) {
  return Array.from({ length: prefix.len }, (_, slot) => {
    const cell =
      prefix.physical < scratchBlocks.value
        ? frame.value.scratch[prefix.physical * b.value + slot]
        : frame.value.input[(prefix.physical - scratchBlocks.value) * b.value + slot];
    return { slot, cell };
  });
}
function chainXY(bucket: number, position: number) {
  const l = layout.value;
  return {
    x: l.left + (position % l.chainPerLine) * l.chainStride,
    y: l.rows[bucket].boxTop + Math.floor(position / l.chainPerLine) * CHAIN_LINE,
  };
}
const chainBoxes = computed(() =>
  frame.value.chains.flatMap((chain, bucket) =>
    chain.map((prefix, position) => {
      const at = chainXY(bucket, position);
      // The link arrow follows the previous block, also across a line break.
      const previous = position > 0 ? chainXY(bucket, position - 1) : null;
      return {
        key: `p${prefix.physical}`,
        bucket,
        prefix,
        ...at,
        arrow: previous && { x: previous.x - at.x + b.value * CW + 4, y: previous.y - at.y },
        records: prefixRecords(prefix),
        jump: prefix.source === undefined ? undefined : sourceFrame.value.get(prefix.source),
      };
    }),
  ),
);

const bandsVisible = computed(() => frame.value.owners !== null || frame.value.phase === "done");
const bands = computed(() => result.value?.bands ?? []);
/** One bar under each input position of a final band, so bands follow wrapped lines. */
const bandCells = computed(() =>
  bands.value.flatMap((band, bucket) =>
    Array.from({ length: band.len }, (_, i) => ({ bucket, ...inputXY(band.start + i) })),
  ),
);
const bandText = (k: number) => {
  const band = bands.value[k];
  if (!bandsVisible.value || !band?.len) return "";
  return band.len === 1 ? `→ ${band.start}` : `→ ${band.start}–${band.start + band.len - 1}`;
};

const hovered = ref<number | null>(null);
const emphasised = (physical: number) =>
  frame.value.focus.includes(physical) || hovered.value === physical;

/**
 * During the scatter each bucket is a singly linked list: its head is its
 * scratch block, every full block names the next one, and the bucket writes
 * at the tail.
 */
const links = computed(() => {
  const next = new Map<number, { bucket: number; to: number }>();
  if (frame.value.writers === null) return next;
  frame.value.chains.forEach((chain, bucket) => {
    for (let i = 1; i < chain.length; i++) {
      next.set(chain[i - 1].physical, { bucket, to: chain[i].physical });
    }
  });
  return next;
});

function role(physical: number): { text: string; color?: string } {
  const f = frame.value;
  const writer = f.writers?.findIndex((w) => w.physical === physical) ?? -1;
  if (writer >= 0) return { text: `${writer} writes`, color: bucketColor(writer, BUCKETS) };
  const link = links.value.get(physical);
  if (link) {
    return { text: `next ${blockName(link.to, scratchBlocks.value)}`, color: bucketColor(link.bucket, BUCKETS) };
  }
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
  <figure ref="figure" class="tsr-figure">
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
        Play speed
        <select v-model.number="stepMs">
          <option :value="3000">slow, 3 s a step</option>
          <option :value="1600">normal, 1.6 s a step</option>
          <option :value="700">fast, 0.7 s a step</option>
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
          <button type="button" class="tsr-button tsr-play" :title="stepper.playing.value ? 'Pause (space)' : 'Play (space)'" @click="stepper.toggle">
            {{ stepper.playing.value ? "Pause" : "Play" }}
          </button>
        </div>
      </div>
      <!-- Every caption occupies the same cell, so the tallest one sets the height for the whole run. -->
      <div class="tsr-captions" aria-live="polite">
        <p v-for="(f, i) in frames" :key="`${run}-${i}`" :class="{ current: f === frame }" :aria-hidden="f !== frame">
          {{ f.caption }}
        </p>
      </div>

      <svg
        :viewBox="`0 0 ${layout.width} ${layout.height}`"
        :width="layout.width"
        :height="layout.height"
        role="img"
        aria-label="The input slice, the scratch blocks, and each bucket's chain of blocks"
      >
        <!-- Row titles: beside the rows, or above them on narrow screens -->
        <template v-if="layout.compact">
          <text x="0" :y="layout.inputTitle + 13" font-size="12"><tspan font-weight="600">Input</tspan> · blocks of b = {{ b }} records</text>
          <text x="0" :y="layout.scratchTitle[0] + 13" font-size="12"><tspan font-weight="600">Scratch</tspan> · each bucket's first block</text>
          <text x="0" :y="layout.scratchTitle[1] + 13" font-size="12"><tspan font-weight="600">Scratch</tspan> · reserve for completion</text>
        </template>
        <template v-else>
          <text x="0" :y="layout.inputTop + 15" font-size="12" font-weight="600">Input</text>
          <text x="0" :y="layout.inputTop + 29" font-size="10" opacity="0.7">b = {{ b }} per block</text>
          <text x="0" :y="layout.scratchTop[0] + 15" font-size="12" font-weight="600">Scratch</text>
          <text x="0" :y="layout.scratchTop[0] + 29" font-size="10" opacity="0.7">first blocks</text>
          <text x="0" :y="layout.scratchTop[1] + 15" font-size="12" font-weight="600">Scratch</text>
          <text x="0" :y="layout.scratchTop[1] + 29" font-size="10" opacity="0.7">reserve</text>
        </template>
        <text x="0" :y="layout.bucketsTitle" font-size="12">
          <tspan font-weight="600">Buckets</tspan> · {{ layout.compact ? "singly linked lists of blocks" : "each is a singly linked list of blocks, head first" }}
        </text>

        <!-- Input positions; a click jumps to the step that reads it. -->
        <g v-for="pos in inputPositions" :key="`i${pos.index}`">
          <!-- biome-ignore lint/a11y/noStaticElementInteractions: pointer shortcuts; the step toolbar and its keys reach every step. -->
          <text
            :x="pos.x + CW / 2"
            :y="pos.y - 5"
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
            :y="pos.y + 1"
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
          <text :x="box.x" :y="box.y - (box.physical < scratchBlocks ? 6 : INDEX + 6)" font-size="11" opacity="0.8">
            {{ blockName(box.physical, scratchBlocks) }}
          </text>
          <!-- A block is b consecutive slots: one tinted unit -->
          <rect :x="box.x - 2" :y="box.y - 2" :width="b * CW + 4" :height="CH + 4" rx="6" fill="var(--vp-c-bg-soft)" />
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
          <text :x="box.x" :y="box.y + CH + (box.physical < scratchBlocks ? 14 : BAND + 14)" font-size="10">
            <tspan v-if="role(box.physical).color" :fill="role(box.physical).color">■ </tspan>
            <tspan :style="{ fill: role(box.physical).color ? 'var(--vp-c-text-1)' : 'var(--vp-c-text-3)' }">{{ role(box.physical).text }}</tspan>
          </text>
        </g>
        <template v-if="tailLen">
          <text :x="tailXY.x" :y="tailXY.y - INDEX - 6" font-size="11" opacity="0.8">tail</text>
          <rect
            :x="tailXY.x - 2"
            :y="tailXY.y - 2"
            :width="tailLen * CW + 4"
            :height="CH + 4"
            rx="6"
            fill="none"
            stroke="var(--tsr-line)"
            stroke-dasharray="4 3"
          />
        </template>

        <!-- Final bands: where each bucket ends up in the input -->
        <g :opacity="bandsVisible ? 1 : 0" class="tsr-fade">
          <rect
            v-for="cell in bandCells"
            :key="`band${cell.x}-${cell.y}`"
            :x="cell.x"
            :y="cell.y + CH + 3"
            :width="CW"
            height="4"
            :fill="bucketColor(cell.bucket, BUCKETS)"
          />
        </g>

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
        <g v-for="(row, k) in layout.rows" :key="`row${k}`">
          <template v-if="layout.compact">
            <rect x="0" :y="row.top + 3" width="10" height="10" rx="2" :fill="bucketColor(k, BUCKETS)" />
            <text x="15" :y="row.top + 12" font-size="12">
              <tspan font-weight="600">bucket {{ k }}</tspan><tspan opacity="0.7"> · tens digit {{ k }} {{ bandText(k) }}</tspan>
            </text>
          </template>
          <template v-else>
            <rect x="0" :y="row.top + NAME + 4" width="10" height="10" rx="2" :fill="bucketColor(k, BUCKETS)" />
            <text x="15" :y="row.top + NAME + 13" font-size="12" font-weight="600">bucket {{ k }}</text>
            <text x="0" :y="row.top + NAME + 28" font-size="10" opacity="0.7">tens digit {{ k }}</text>
            <text x="0" :y="row.top + NAME + 42" font-size="10" opacity="0.7">{{ bandText(k) }}</text>
          </template>
          <text :x="layout.left" :y="row.boxTop + 21" font-size="11" opacity="0.5">
            {{ frame.chains[k]?.length ? "" : frame.phase === "setup" ? "—" : bandsVisible ? "complete" : "empty" }}
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
          <text v-if="box.arrow" :x="box.arrow.x" :y="box.arrow.y + CH / 2 + 5" font-size="13" opacity="0.6">→</text>
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
        <span>Tap a position or a ledger block to jump there; ← → step.</span>
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
  flex-wrap: wrap;
  gap: 4px;
  margin: 0;
  padding: 0;
  list-style: none;
}

/* The page styles space consecutive list items apart; the phases sit on one line. */
.tsr-phases li {
  margin: 0;
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

.tsr-play {
  min-width: 60px;
}

.tsr-captions {
  display: grid;
  margin: 10px 0 12px;
  font-size: 14px;
  line-height: 1.5;
}

.tsr-captions p {
  grid-area: 1 / 1;
  margin: 0;
  visibility: hidden;
}

.tsr-captions p.current {
  visibility: visible;
}

.tsr-fade {
  transition: opacity 0.3s ease;
}

.tsr-figure svg {
  max-width: none;
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
  .tsr-move,
  .tsr-fade {
    transition: none;
  }
}

@media (max-width: 640px) {
  .tsr-figure {
    padding: 12px;
  }
}
</style>
