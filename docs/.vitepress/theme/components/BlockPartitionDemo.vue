<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { computed, onMounted, ref, shallowRef, watch } from "vue";
import { bucketColor } from "../palette";
import { blockName, type Frame, type Phase, simulate } from "../sim/blockPartition";
import { type Engine, instantiate } from "../sim/engine";
import wasmUrl from "../sim/tessera_trace.wasm?url";

// Replays Tessera's real Rust block partition (compiled to WebAssembly) on keys 0–39 (4 buckets).
const RADIX = 2;
const BUCKETS = 1 << RADIX;
const RANGE = ["0–9", "10–19", "20–29", "30–39"] as const;
const DIVISOR = 10;

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
  return Array.from({ length: n }, () => {
    const r = next();
    const bucket = r < 0.4 ? 1 : r < 0.6 ? 0 : r < 0.85 ? 3 : 2;
    return bucket * 10 + Math.floor(next() * 10);
  });
}

const result = computed(() =>
  engine.value
    ? simulate(engine.value, {
        keys: makeKeys(length.value, seed.value),
        radix: RADIX,
        blockSize: blockSize.value,
        divisor: DIVISOR,
      })
    : null,
);

const frames = computed(() => result.value?.frames ?? []);
const frameIndex = ref(0);
const hoveredBucket = ref<number | null>(null);
const hoveredBlock = ref<number | null>(null);

const phaseBounds = computed(() => {
  const find = (wanted: Phase[]) => {
    const first = frames.value.findIndex((f) => wanted.includes(f.phase));
    let last = first;
    for (let i = frames.value.length - 1; i >= 0; i--) {
      if (wanted.includes(frames.value[i].phase)) {
        last = i;
        break;
      }
    }
    return { first: Math.max(0, first), last: Math.max(0, last) };
  };
  return {
    scatter: find(["setup", "scatter"]),
    ledger: find(["ledger"]),
    complete: find(["complete", "done"]),
  };
});

// Start on a revealing mid-scatter frame where input recycling is already happening.
watch(
  frames,
  (list) => {
    if (!list.length) return;
    const { first, last } = phaseBounds.value.scatter;
    frameIndex.value = Math.min(last, first + Math.max(1, Math.floor((last - first) * 0.65)));
  },
  { immediate: true },
);

const frame = computed<Frame | null>(() => frames.value[frameIndex.value] ?? null);

type MacroPhase = "scatter" | "ledger" | "complete";
const activePhase = computed<MacroPhase>(() => {
  const p = frame.value?.phase;
  if (p === "ledger") return "ledger";
  if (p === "complete" || p === "done") return "complete";
  return "scatter";
});

function jumpPhase(phase: MacroPhase) {
  const b = phaseBounds.value[phase];
  frameIndex.value = phase === "complete" ? b.last : phase === "scatter" ? b.first : b.first;
}

const readThrough = computed(() => {
  const out: number[] = [];
  let seen = 0;
  for (const f of frames.value) {
    if (f.phase === "setup") seen = 0;
    else if (f.phase === "scatter") {
      if (f.reads !== undefined) seen = f.reads + 1;
    } else seen = length.value;
    out.push(seen);
  }
  return out;
});

const readIndex = computed(() => readThrough.value[frameIndex.value] ?? 0);

// Map each input slot (including tail) to the bucket that will own it in the final layout.
const slotFinalBucket = computed(() => {
  const owners = new Array<number | null>(length.value).fill(null);
  if (!result.value) return owners;
  result.value.bands.forEach((band, bucket) => {
    for (let i = band.start; i < band.start + band.len; i++) owners[i] = bucket;
  });
  return owners;
});

// Input blocks strictly before `readBlocks` have had every record read and are free to recycle.
const readBlocks = computed(() => Math.floor(readIndex.value / blockSize.value));

const physicalBlocks = computed(() => {
  if (!result.value || !frame.value) return [];
  const { scratchBlocks, fullBlocks } = result.value;
  const b = blockSize.value;
  const f = frame.value;
  const blockOwnerBucket = new Map<number, number>();
  f.chains.forEach((chain, bucket) => {
    for (const prefix of chain) blockOwnerBucket.set(prefix.physical, bucket);
  });
  const out = [];
  for (let p = 0; p < scratchBlocks + fullBlocks; p++) {
    const isScratch = p < scratchBlocks;
    const inputIdx = p - scratchBlocks;
    const cells = isScratch
      ? f.scratch.slice(p * b, (p + 1) * b)
      : f.input.slice(inputIdx * b, (inputIdx + 1) * b);
    const owner = f.owners ? f.owners[p] : null;
    const bucket = typeof owner === "number" ? owner : (blockOwnerBucket.get(p) ?? null);
    const recycled = !isScratch && activePhase.value === "scatter" && inputIdx < readBlocks.value;
    const isReserve = isScratch && p >= BUCKETS;
    const role =
      owner === "final"
        ? "final"
        : owner === "free"
          ? "free"
          : typeof owner === "number"
            ? `bucket ${owner}`
            : recycled
              ? bucket !== null
                ? `recycled → B${bucket}`
                : "read & free"
              : isReserve
                ? "reserve"
                : isScratch
                  ? `head B${p}`
                  : "unread";
    out.push({
      physical: p,
      name: blockName(p, scratchBlocks),
      isScratch,
      isReserve,
      inputIdx,
      cells,
      bucket,
      recycled,
      role,
      focused: f.focus.includes(p),
    });
  }
  return out;
});

const tailCells = computed(() => {
  if (!result.value || !frame.value) return [];
  return frame.value.input.slice(result.value.fullBlocks * blockSize.value);
});

const sliderBounds = computed(() => {
  const b = phaseBounds.value[activePhase.value];
  return { min: b.first, max: b.last };
});

function blockDimmed(physical: number, bucket: number | null) {
  if (hoveredBlock.value !== null) return hoveredBlock.value !== physical;
  if (hoveredBucket.value !== null) return hoveredBucket.value !== bucket;
  return false;
}

function jumpToRead(inputPos: number) {
  const target = frames.value.findIndex((f) => f.reads === inputPos);
  if (target >= 0) frameIndex.value = target;
}

function jumpToSource(sourceId?: number) {
  if (sourceId === undefined) return;
  const target = frames.value.findIndex((f) => f.source === sourceId);
  if (target >= 0) frameIndex.value = target;
}
</script>

<template>
  <figure class="tsr-figure bp-root">
    <p v-if="failed" class="tsr-note">The WebAssembly engine replay could not be loaded.</p>
    <p v-else-if="!result || !frame" class="tsr-note">Loading WebAssembly engine…</p>
    <template v-else>
      <div class="bp-top">
        <div class="tsr-seg" role="tablist" aria-label="Block partition phase">
          <button
            v-for="[id, label] in [
              ['scatter', '1. Scatter & recycle'],
              ['ledger', '2. Bucket chains'],
              ['complete', '3. Write back'],
            ] as const"
            :key="id"
            type="button"
            role="tab"
            :aria-selected="activePhase === id"
            :class="{ active: activePhase === id }"
            @click="jumpPhase(id)"
          >
            {{ label }}
          </button>
        </div>

        <div class="bp-meta">
          <label>
            Block size
            <select v-model.number="blockSize" aria-label="Block size">
              <option :value="2">2</option>
              <option :value="3">3</option>
              <option :value="4">4</option>
            </select>
          </label>
          <button type="button" class="bp-shuffle" @click="seed++">New input</button>
        </div>
      </div>

      <div v-if="sliderBounds.max > sliderBounds.min" class="bp-scrub">
        <span class="bp-scrub-label">
          {{
            activePhase === 'scatter'
              ? `Read frontier: ${readIndex} / ${length} records (${readBlocks} input blocks freed)`
              : `Write-back progress: step ${frameIndex - sliderBounds.min + 1} / ${sliderBounds.max - sliderBounds.min + 1}`
          }}
        </span>
        <input
          type="range"
          :min="sliderBounds.min"
          :max="sliderBounds.max"
          v-model.number="frameIndex"
          :aria-label="activePhase === 'scatter' ? 'Scatter progress' : 'Write-back progress'"
        />
      </div>

      <!-- Physical memory: Input array + Small scratch blocks -->
      <div class="bp-section-label">
        <span>Input slice ({{ length }} records)</span>
        <span v-if="activePhase === 'scatter'" class="bp-hint">
          Click any input slot to jump there · blocks left of the frontier are already read and recycled
        </span>
        <span v-else class="bp-hint">
          Coloured underlines mark each bucket's final contiguous range
        </span>
      </div>

      <div class="bp-blocks">
        <!-- biome-ignore lint/a11y/noStaticElementInteractions: hover highlights a physical block across views; inner cell buttons provide keyboard navigation. -->
        <div
          v-for="blk in physicalBlocks.filter((x) => !x.isScratch)"
          :key="blk.physical"
          class="bp-block"
          :class="{
            recycled: blk.recycled,
            focused: blk.focused,
            dimmed: blockDimmed(blk.physical, blk.bucket),
            frontier: activePhase === 'scatter' && blk.inputIdx === readBlocks,
          }"
          @mouseenter="hoveredBlock = blk.physical"
          @mouseleave="hoveredBlock = null"
        >
          <div class="bp-block-head">
            <span class="bp-block-name">{{ blk.name }}</span>
            <span class="bp-block-role" :class="{ good: blk.recycled || blk.role === 'final' }">{{ blk.role }}</span>
          </div>
          <div class="bp-cells">
            <button
              v-for="(cell, offset) in blk.cells"
              :key="offset"
              type="button"
              class="bp-cell"
              :class="{
                stale: !cell.live,
                empty: !cell.rec,
                banded: activePhase !== 'scatter' && slotFinalBucket[blk.inputIdx * blockSize + offset] !== null,
              }"
              :style="{
                background: cell.rec && cell.live ? bucketColor(cell.rec.bucket, BUCKETS) : undefined,
                borderBottomColor:
                  activePhase !== 'scatter' && slotFinalBucket[blk.inputIdx * blockSize + offset] !== null
                    ? bucketColor(slotFinalBucket[blk.inputIdx * blockSize + offset]!, BUCKETS)
                    : undefined,
              }"
              @click="jumpToRead(blk.inputIdx * blockSize + offset)"
            >
              <template v-if="cell.rec">
                <b>{{ cell.rec.key }}</b>
                <small>#{{ cell.rec.id }}</small>
              </template>
            </button>
          </div>
        </div>

        <div v-if="tailCells.length" class="bp-block tail">
          <div class="bp-block-head">
            <span class="bp-block-name">Tail</span>
            <span class="bp-block-role">partial</span>
          </div>
          <div class="bp-cells">
            <button
              v-for="(cell, offset) in tailCells"
              :key="offset"
              type="button"
              class="bp-cell"
              :class="{
                stale: !cell.live,
                empty: !cell.rec,
                banded: activePhase !== 'scatter' && slotFinalBucket[result.fullBlocks * blockSize + offset] !== null,
              }"
              :style="{
                background: cell.rec && cell.live ? bucketColor(cell.rec.bucket, BUCKETS) : undefined,
                borderBottomColor:
                  activePhase !== 'scatter' && slotFinalBucket[result.fullBlocks * blockSize + offset] !== null
                    ? bucketColor(slotFinalBucket[result.fullBlocks * blockSize + offset]!, BUCKETS)
                    : undefined,
              }"
              @click="jumpToRead(result.fullBlocks * blockSize + offset)"
            >
              <template v-if="cell.rec">
                <b>{{ cell.rec.key }}</b>
                <small>#{{ cell.rec.id }}</small>
              </template>
            </button>
          </div>
        </div>
      </div>

      <div class="bp-section-label">
        <span>Extra scratch ({{ result.scratchBlocks }} blocks: {{ BUCKETS }} bucket heads + {{ BUCKETS }} completion reserve)</span>
      </div>

      <div class="bp-blocks">
        <!-- biome-ignore lint/a11y/noStaticElementInteractions: hover highlights a scratch block across views; bucket chain buttons below provide keyboard focus. -->
        <div
          v-for="blk in physicalBlocks.filter((x) => x.isScratch)"
          :key="blk.physical"
          class="bp-block scratch"
          :class="{
            reserve: blk.isReserve,
            focused: blk.focused,
            dimmed: blockDimmed(blk.physical, blk.bucket),
          }"
          @mouseenter="hoveredBlock = blk.physical"
          @mouseleave="hoveredBlock = null"
        >
          <div class="bp-block-head">
            <span class="bp-block-name">{{ blk.name }}</span>
            <span class="bp-block-role">{{ blk.role }}</span>
          </div>
          <div class="bp-cells">
            <span
              v-for="(cell, offset) in blk.cells"
              :key="offset"
              class="bp-cell"
              :class="{ stale: !cell.live, empty: !cell.rec }"
              :style="cell.rec && cell.live ? { background: bucketColor(cell.rec.bucket, BUCKETS) } : undefined"
            >
              <template v-if="cell.rec">
                <b>{{ cell.rec.key }}</b>
                <small>#{{ cell.rec.id }}</small>
              </template>
            </span>
          </div>
        </div>
      </div>

      <!-- Logical bucket chains -->
      <div class="bp-section-label">
        <span>Logical bucket chains (hover a bucket to trace its physical blocks)</span>
      </div>

      <div class="bp-chains">
        <!-- biome-ignore lint/a11y/noStaticElementInteractions: hover highlights a bucket's physical blocks; inner block pills provide keyboard focus. -->
        <div
          v-for="(chain, b) in frame.chains"
          :key="b"
          class="bp-chain-row"
          :class="{ dimmed: hoveredBucket !== null && hoveredBucket !== b }"
          @mouseenter="hoveredBucket = b"
          @mouseleave="hoveredBucket = null"
        >
          <span class="bp-bucket-tag" :style="{ borderColor: bucketColor(b, BUCKETS) }">
            <i class="bp-dot" :style="{ background: bucketColor(b, BUCKETS) }" />
            {{ RANGE[b] }}
            <small v-if="result.bands[b].len">
              → [{{ result.bands[b].start }}..{{ result.bands[b].start + result.bands[b].len - 1 }}]
            </small>
          </span>
          <div class="bp-chain-links">
            <span v-if="!chain.length" class="bp-empty-chain">
              {{ activePhase === 'complete' ? 'copied to final band ✓' : 'empty' }}
            </span>
            <template v-for="(prefix, idx) in chain" :key="`${prefix.physical}-${idx}`">
              <span v-if="idx > 0" class="bp-arrow">→</span>
              <button
                type="button"
                class="bp-link-pill"
                :class="{ active: hoveredBlock === prefix.physical }"
                @mouseenter="hoveredBlock = prefix.physical"
                @mouseleave="hoveredBlock = null"
                @click="jumpToSource(prefix.source)"
              >
                <strong>{{ blockName(prefix.physical, result.scratchBlocks) }}</strong>
                <span>({{ prefix.len }}/{{ blockSize }})</span>
              </button>
            </template>
          </div>
        </div>
      </div>

      <p class="tsr-caption" aria-live="polite">{{ frame.caption }}</p>
    </template>
  </figure>
</template>

<style scoped>
.bp-top {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 10px 14px;
  margin-bottom: 12px;
}
.bp-meta {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 12.5px;
  color: var(--vp-c-text-2);
}
.bp-meta select,
.bp-shuffle {
  margin-left: 4px;
  padding: 3px 9px;
  border: 1px solid var(--tsr-line);
  border-radius: 6px;
  background: var(--vp-c-bg);
  color: var(--vp-c-text-1);
  font-size: 12.5px;
  cursor: pointer;
}
.bp-scrub {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 14px;
  margin-bottom: 14px;
  padding: 8px 12px;
  border: 1px solid var(--tsr-line);
  border-radius: 8px;
  background: var(--vp-c-bg);
  font-size: 12.5px;
}
.bp-scrub-label {
  color: var(--vp-c-text-2);
  font-family: var(--vp-font-family-mono);
}
.bp-scrub input {
  flex: 1 1 200px;
  accent-color: var(--vp-c-brand-1);
}
.bp-section-label {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  gap: 4px 12px;
  margin: 12px 0 6px;
  font-size: 12px;
  font-weight: 600;
  color: var(--vp-c-text-2);
}
.bp-hint {
  font-weight: 400;
  opacity: 0.8;
}
.bp-blocks {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.bp-block {
  padding: 6px 7px;
  border: 1px solid var(--tsr-line);
  border-radius: 8px;
  background: var(--vp-c-bg);
  transition: opacity 0.18s ease, border-color 0.18s ease;
}
.bp-block.recycled {
  border-color: #1baf7a;
}
.bp-block.frontier {
  border-left: 3px solid var(--tsr-focus);
}
.bp-block.focused {
  border-color: var(--vp-c-brand-1);
  box-shadow: 0 0 0 1px var(--vp-c-brand-1);
}
.bp-block.reserve {
  border-style: dashed;
}
.bp-block.dimmed {
  opacity: 0.3;
}
.bp-block-head {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 5px;
  font-size: 10.5px;
  font-family: var(--vp-font-family-mono);
}
.bp-block-name {
  font-weight: 700;
  color: var(--vp-c-text-1);
}
.bp-block-role {
  color: var(--vp-c-text-3);
}
.bp-block-role.good {
  color: #1baf7a;
}
.bp-cells {
  display: flex;
  gap: 3px;
}
.bp-cell {
  display: inline-flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 34px;
  padding: 0;
  border: 1px solid transparent;
  border-radius: 5px;
  color: var(--tsr-ink);
  font-family: var(--vp-font-family-mono);
  line-height: 1.1;
  cursor: pointer;
}
.bp-cell b {
  font-size: 12px;
}
.bp-cell small {
  font-size: 8.5px;
  opacity: 0.78;
}
.bp-cell.stale {
  background: var(--tsr-stale) !important;
  color: var(--vp-c-text-3);
  opacity: 0.55;
}
.bp-cell.banded {
  border-bottom-width: 4px;
  border-bottom-style: solid;
}
.bp-cell.empty {
  background: transparent !important;
  border: 1px dashed var(--tsr-line);
  cursor: default;
}
.bp-chains {
  display: grid;
  gap: 6px;
}
.bp-chain-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  border: 1px solid var(--tsr-line);
  border-radius: 8px;
  background: var(--vp-c-bg);
  transition: opacity 0.18s ease;
}
.bp-chain-row.dimmed {
  opacity: 0.32;
}
.bp-bucket-tag {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-width: 128px;
  font-size: 12px;
  font-weight: 600;
  font-family: var(--vp-font-family-mono);
}
.bp-bucket-tag small {
  font-weight: 400;
  color: var(--vp-c-text-3);
}
.bp-dot {
  width: 9px;
  height: 9px;
  border-radius: 2px;
}
.bp-chain-links {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  font-family: var(--vp-font-family-mono);
}
.bp-arrow {
  color: var(--vp-c-text-3);
}
.bp-empty-chain {
  color: var(--vp-c-text-3);
  font-style: italic;
}
.bp-link-pill {
  display: inline-flex;
  gap: 4px;
  padding: 2px 8px;
  border: 1px solid var(--tsr-line);
  border-radius: 999px;
  background: var(--tsr-panel);
  color: var(--vp-c-text-1);
  font-size: 11.5px;
  cursor: pointer;
}
.bp-link-pill.active,
.bp-link-pill:hover {
  border-color: var(--vp-c-brand-1);
  background: var(--vp-c-brand-soft);
}
</style>
