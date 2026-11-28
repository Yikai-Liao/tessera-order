<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { computed, ref } from "vue";
import { bucketColor } from "../palette";
import { chooseTileLeaf, type Digit, planDigits } from "../sim/digits";

const BITS = 32;
const RECORDS = 8192;
const varying = ref((1n << 20n) - 1n);
const width = ref(14);
const simdMsd = ref(true);

const presets: [string, bigint][] = [
  ["20-bit range", (1n << 20n) - 1n],
  ["25-bit range", (1n << 25n) - 1n],
  ["All 32 bits", (1n << 32n) - 1n],
  ["Two bytes + gap", 0xff00ffn],
  ["Scattered", 0x80f0_0f01n],
];

const tileLeaf = computed(() =>
  chooseTileLeaf(RECORDS, varying.value, width.value, BITS, 4, simdMsd.value),
);
const tileLeafLabel = computed(
  () =>
    ({
      done: "done (all equal)",
      insertion: "insertion sort",
      msd: "Tile MSD (first split)",
      lsd: "flat LSD",
    })[tileLeaf.value],
);
const plan = computed(() => planDigits(varying.value, width.value, BITS, RECORDS, simdMsd.value));
const fixed = computed<Digit[]>(() => {
  const out: Digit[] = [];
  let remaining = varying.value;
  while (remaining !== 0n) {
    const high = remaining.toString(2).length - 1;
    const low = Math.max(0, high + 1 - width.value);
    out.push({ shift: low, width: high - low + 1 });
    remaining &= ~(((1n << BigInt(high - low + 1)) - 1n) << BigInt(low));
  }
  return out;
});
const slots = (digits: Digit[]) => digits.reduce((sum, d) => sum + 2 ** d.width, 0);
const toggle = (bit: number) => {
  varying.value ^= 1n << BigInt(bit);
};
const isSet = (bit: number) => ((varying.value >> BigInt(bit)) & 1n) === 1n;

const CW = 19;
const LEFT = 74;
const svgWidth = LEFT + BITS * CW + 8;
const xOf = (bit: number) => LEFT + (BITS - 1 - bit) * CW;
</script>

<template>
  <figure class="tsr-figure">
    <div class="dp-top">
      <fieldset class="tsr-seg" aria-label="Varying bit presets">
        <button
          v-for="[name, mask] in presets"
          :key="name"
          type="button"
          :class="{ active: varying === mask }"
          @click="varying = mask"
        >
          {{ name }}
        </button>
      </fieldset>
      <div class="dp-opts">
        <label>
          Max digit: <strong>{{ width }}b</strong>
          <input v-model.number="width" type="range" min="6" max="14" />
        </label>
        <label>
          <input v-model="simdMsd" type="checkbox" />
          SIMD MSD (8+ lanes)
        </label>
      </div>
    </div>

    <svg :viewBox="`0 0 ${svgWidth} 166`" :width="svgWidth" role="img" aria-label="Digit plan over 32 key bits">
      <text x="0" y="24" font-size="11" font-weight="600">varying</text>
      <text x="0" y="36" font-size="9" opacity="0.6">click bits</text>
      <!-- biome-ignore lint/a11y/useSemanticElements: SVG has no `<button>`; the group takes its role. -->
      <g
        v-for="bit in BITS"
        :key="bit"
        role="button"
        tabindex="0"
        :aria-label="`Toggle bit ${bit - 1}`"
        :aria-pressed="isSet(bit - 1)"
        style="cursor: pointer"
        @click="toggle(bit - 1)"
        @keydown.enter.space.prevent="toggle(bit - 1)"
      >
        <rect
          :x="xOf(bit - 1) + 1"
          y="12"
          :width="CW - 2"
          height="26"
          rx="4"
          :fill="isSet(bit - 1) ? 'var(--vp-c-brand-soft)' : 'transparent'"
          :stroke="isSet(bit - 1) ? 'var(--vp-c-brand-1)' : 'var(--tsr-line)'"
        />
        <text :x="xOf(bit - 1) + CW / 2" y="29" text-anchor="middle" font-size="11">{{ isSet(bit - 1) ? 1 : 0 }}</text>
        <text v-if="(bit - 1) % 4 === 0" :x="xOf(bit - 1) + CW / 2" y="51" text-anchor="middle" font-size="9" opacity="0.55">
          {{ bit - 1 }}
        </text>
      </g>

      <template v-for="(row, r) in [{ name: 'Tessera', digits: plan }, { name: 'Fixed', digits: fixed }]" :key="row.name">
        <text x="0" :y="84 + r * 48" font-size="11" font-weight="600">{{ row.name }}</text>
        <g v-for="(d, i) in row.digits" :key="`${row.name}${i}`">
          <rect
            :x="xOf(d.shift + d.width - 1) + 1"
            :y="68 + r * 48"
            :width="d.width * CW - 2"
            height="26"
            rx="6"
            :fill="bucketColor(i, Math.max(row.digits.length, 2))"
            opacity="0.88"
          />
          <text
            :x="xOf(d.shift + d.width - 1) + (d.width * CW) / 2"
            :y="85 + r * 48"
            text-anchor="middle"
            font-size="11"
            font-weight="600"
            style="fill: var(--tsr-ink)"
          >
            {{
              row.name === 'Tessera' && tileLeaf === 'msd'
                ? d.width > 1
                  ? `split 1 · ${d.width}b`
                  : '1'
                : d.width > 1
                  ? `pass ${row.digits.length - i} · ${d.width}b`
                  : row.digits.length - i
            }}
          </text>
        </g>
      </template>
    </svg>

    <div class="dp-summary">
      <div><span>Selected leaf (8,192 u32)</span><strong>{{ tileLeafLabel }}</strong></div>
      <div>
        <span>{{ tileLeaf === 'msd' ? 'Tessera first split (MSD)' : 'Tessera plan' }}</span>
        <strong>
          {{ [...plan].reverse().map((d) => `${d.width}b`).join(' + ') || 'none' }}
          ({{ slots(plan).toLocaleString('en-US') }} slots<template v-if="tileLeaf === 'msd'">, first split; unfinished buckets re-plan</template>)
        </strong>
      </div>
      <div>
        <span>Fixed {{ width }}b windows</span>
        <strong>{{ [...fixed].reverse().map((d) => `${d.width}b`).join(' + ') || 'none' }} ({{ slots(fixed).toLocaleString('en-US') }} slots)</strong>
      </div>
    </div>
  </figure>
</template>

<style scoped>
.dp-top {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 10px 14px;
  margin-bottom: 10px;
}
.dp-opts {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px;
  font-size: 12.5px;
  color: var(--vp-c-text-2);
}
.dp-opts label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.dp-summary {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
  gap: 8px;
  margin-top: 12px;
}
.dp-summary div {
  padding: 8px 10px;
  border: 1px solid var(--tsr-line);
  border-radius: 8px;
  background: var(--vp-c-bg);
}
.dp-summary span {
  display: block;
  font-size: 11px;
  color: var(--vp-c-text-3);
}
.dp-summary strong {
  font-size: 13px;
  font-family: var(--vp-font-family-mono);
}
</style>
