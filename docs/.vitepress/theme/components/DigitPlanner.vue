<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { computed, ref } from "vue";
import { bucketColor } from "../palette";
import { chooseTileLeaf, type Digit, planDigits } from "../sim/digits";

const BITS = 32;
const varying = ref((1n << 32n) - 1n);
const width = ref(14);
const sampledDistinct = ref(2);

const tileLeaf = computed(() => chooseTileLeaf(8192, varying.value, width.value, sampledDistinct.value, BITS));
const tileLeafLabel = computed(() =>
  ({ done: "already ordered", insertion: "insertion sort", msd: "MSD", lsd: "LSD" })[tileLeaf.value],
);
const presets: [string, bigint][] = [
  ["all 32 bits", (1n << 32n) - 1n],
  ["low 20 bits", (1n << 20n) - 1n],
  ["two bytes with a gap", 0xff00ffn],
  ["scattered", 0x80f0_0f01n],
];

function toggle(bit: number) {
  varying.value ^= 1n << BigInt(bit);
}
const isSet = (bit: number) => ((varying.value >> BigInt(bit)) & 1n) === 1n;

const plan = computed(() => planDigits(varying.value, width.value, BITS));
const tileLeafReason = computed(() => {
  if (varying.value === 0n) return "No key bits vary, so no pass is needed.";
  if (plan.value.length <= 2) return "The plan needs at most two LSD passes, so the engine skips sampling.";
  if (sampledDistinct.value <= 2) return "The low-fanout sample selects MSD directly.";
  return "The estimated MSD cost is compared with the LSD pass count.";
});
/** Fixed windows of the full width from the top bit down, for comparison. */
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

const CW = 19;
const LEFT = 80;
const svgWidth = LEFT + BITS * CW + 8;
const xOf = (bit: number) => LEFT + (BITS - 1 - bit) * CW;
</script>

<template>
  <figure class="tsr-figure">
    <div class="tsr-input">
      <label>
        Widest digit (cursor table): {{ width }} bits
        <input v-model.number="width" type="range" min="6" max="14" />
      </label>
      <label>
        Distinct top-digit buckets in sample: {{ sampledDistinct }}
        <input v-model.number="sampledDistinct" type="range" min="1" max="128" />
      </label>
      <span>Presets:</span>
      <button v-for="[name, mask] in presets" :key="name" type="button" class="preset" @click="varying = mask">
        {{ name }}
      </button>
    </div>
    <svg :viewBox="`0 0 ${svgWidth} 168`" :width="svgWidth" role="img" aria-label="Digit plan over 32 key bits">
      <text x="0" y="24" font-size="11">varying</text>
      <text x="0" y="36" font-size="9" opacity="0.6">click a bit</text>
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
          rx="3"
          :fill="isSet(bit - 1) ? 'var(--vp-c-brand-soft)' : 'transparent'"
          :stroke="isSet(bit - 1) ? 'var(--vp-c-brand-1)' : 'var(--tsr-line)'"
        />
        <text :x="xOf(bit - 1) + CW / 2" y="30" text-anchor="middle" font-size="11">{{ isSet(bit - 1) ? 1 : 0 }}</text>
        <text v-if="(bit - 1) % 4 === 0" :x="xOf(bit - 1) + CW / 2" y="52" text-anchor="middle" font-size="9" opacity="0.6">
          {{ bit - 1 }}
        </text>
      </g>

      <template v-for="(row, r) in [{ name: 'Tessera', digits: plan }, { name: 'fixed', digits: fixed }]" :key="row.name">
        <text x="0" :y="84 + r * 50" font-size="11">{{ row.name }}</text>
        <g v-for="(d, i) in row.digits" :key="`${row.name}${i}`">
          <rect
            :x="xOf(d.shift + d.width - 1) + 1"
            :y="68 + r * 50"
            :width="d.width * CW - 2"
            height="26"
            rx="5"
            :fill="bucketColor(i, Math.max(row.digits.length, 2))"
            opacity="0.85"
          />
          <text
            :x="xOf(d.shift + d.width - 1) + (d.width * CW) / 2"
            :y="85 + r * 50"
            text-anchor="middle"
            font-size="11"
            font-weight="600"
            style="fill: var(--tsr-ink)"
          >
            {{ d.width > 1 ? `pass ${row.digits.length - i} · ${d.width}b` : row.digits.length - i }}
          </text>
        </g>
      </template>
    </svg>
    <table class="tsr-table">
      <thead>
        <tr><th>Plan</th><th>Passes</th><th>Digit widths, in LSD order</th><th>Bucket slots cleared and scanned</th></tr>
      </thead>
      <tbody>
        <tr>
          <td>Tessera (<code>plan_digits</code>)</td>
          <td>{{ plan.length }}</td>
          <td>{{ [...plan].reverse().map((d) => d.width).join(' + ') || '—' }}</td>
          <td>{{ slots(plan).toLocaleString('en-US') }}</td>
        </tr>
        <tr>
          <td>Fixed windows of {{ width }} bits</td>
          <td>{{ fixed.length }}</td>
          <td>{{ [...fixed].reverse().map((d) => d.width).join(' + ') || '—' }}</td>
          <td>{{ slots(fixed).toLocaleString('en-US') }}</td>
        </tr>
      </tbody>
    </table>
    <p class="tsr-note">
      Passes run from the lowest window (pass 1) to the highest. A window may contain constant bits between varying ones,
      but never starts or ends on one.
    </p>
    <p class="tsr-note">
      For an 8,192-key <code>u64</code> tile, this sample selects <strong>{{ tileLeafLabel }}</strong>.
      {{ tileLeafReason }}
    </p>
  </figure>
</template>

<style scoped>
.preset {
  padding: 1px 9px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 999px;
  font-size: 12px;
}
.preset:hover {
  border-color: var(--vp-c-brand-1);
}
</style>
