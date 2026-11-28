<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { computed, ref } from "vue";
import { planDigits } from "../sim/digits";

const PRESETS = [
  ["Shared prefix", "0x1A40, 0x1A48, 0x1B40, 0x1A4C, 0x1B44"],
  ["Sparse flags", "0x4001, 0x0001, 0x4000, 0x0000"],
  ["All equal", "0x1A40, 0x1A40, 0x1A40, 0x1A40"],
] as const;

const text = ref<string>(PRESETS[0][1]);
const BITS = 16;

const keys = computed(() =>
  text.value
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean)
    .map((s) => Number(s))
    .filter((v) => Number.isInteger(v) && v >= 0 && v < 2 ** BITS),
);
const union = computed(() => keys.value.reduce((a, k) => a | k, 0));
const intersection = computed(() => keys.value.reduce((a, k) => a & k, 2 ** BITS - 1));
const varying = computed(() => (keys.value.length ? union.value ^ intersection.value : 0));
const count = computed(
  () =>
    varying.value
      .toString(2)
      .split("")
      .filter((c) => c === "1").length,
);
const passes = computed(() => planDigits(BigInt(varying.value), 8, BITS).length);
const bitsOf = (v: number) => v.toString(2).padStart(BITS, "0").split("");
const isVarying = (i: number) => ((varying.value >> (BITS - 1 - i)) & 1) === 1;
</script>

<template>
  <figure class="tsr-figure">
    <fieldset class="tsr-seg" aria-label="Key presets">
      <button
        v-for="[label, value] in PRESETS"
        :key="label"
        type="button"
        :class="{ active: text === value }"
        @click="text = value"
      >
        {{ label }}
      </button>
    </fieldset>
    <div class="tsr-input">
      <textarea v-model="text" rows="1" aria-label="16-bit keys, separated by commas"></textarea>
    </div>
    <table class="tsr-table">
      <tbody>
        <tr v-for="(key, row) in keys" :key="row">
          <td>key {{ row + 1 }}</td>
          <td class="tsr-bits">
            <span
              v-for="(bit, i) in bitsOf(key)"
              :key="i"
              :class="{ vary: isVarying(i), flat: !isVarying(i), sep: i > 0 && i % 4 === 0 }"
            >{{ bit }}</span>
          </td>
        </tr>
        <tr class="sum">
          <td>union (OR)</td>
          <td class="tsr-bits">
            <span v-for="(bit, i) in bitsOf(union)" :key="i" :class="{ sep: i > 0 && i % 4 === 0 }">{{ bit }}</span>
          </td>
        </tr>
        <tr>
          <td>intersection (AND)</td>
          <td class="tsr-bits">
            <span v-for="(bit, i) in bitsOf(intersection)" :key="i" :class="{ sep: i > 0 && i % 4 === 0 }">{{ bit }}</span>
          </td>
        </tr>
        <tr>
          <td><strong>varying = OR ^ AND</strong></td>
          <td class="tsr-bits">
            <span
              v-for="(bit, i) in bitsOf(varying)"
              :key="i"
              :class="{ vary: isVarying(i), flat: !isVarying(i), sep: i > 0 && i % 4 === 0 }"
            >{{ bit }}</span>
          </td>
        </tr>
      </tbody>
    </table>
    <p class="tsr-caption">
      <strong>{{ count }} of {{ BITS }} bits vary</strong> (constant columns are dimmed and skipped).
      {{
        count === 0
          ? "Zero varying bits means every key is equal — done in one scan, zero passes."
          : `With 8-bit digits, covering only the varying bits needs ${passes} ${passes === 1 ? "pass" : "passes"} instead of 2.`
      }}
    </p>
  </figure>
</template>

<style scoped>
.vary {
  border-radius: 2px;
  background: var(--vp-c-brand-soft);
  color: var(--vp-c-brand-1);
  font-weight: 700;
}
.flat {
  opacity: 0.35;
}
.sep {
  margin-left: 6px;
}
.sum td {
  border-top: 2px solid var(--vp-c-divider);
}
</style>
