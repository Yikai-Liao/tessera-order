<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { computed, ref } from 'vue'
import { planDigits } from '../sim/digits'

// `varying_key_bits` in block.rs: a bit varies if it is set in the union of
// all keys but not in their intersection.
const text = ref('0x1A40, 0x1A48, 0x1B40, 0x1A4C, 0x1B44')
const BITS = 16

const keys = computed(() =>
  text.value
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean)
    .map((s) => Number(s))
    .filter((v) => Number.isInteger(v) && v >= 0 && v < 2 ** BITS),
)
const union = computed(() => keys.value.reduce((a, k) => a | k, 0))
const intersection = computed(() => keys.value.reduce((a, k) => a & k, 2 ** BITS - 1))
const varying = computed(() => (keys.value.length ? union.value ^ intersection.value : 0))
const count = computed(() => varying.value.toString(2).split('').filter((c) => c === '1').length)
const passes = computed(() => planDigits(BigInt(varying.value), 8, BITS).length)
const bitsOf = (v: number) => v.toString(2).padStart(BITS, '0').split('')
const isVarying = (i: number) => ((varying.value >> (BITS - 1 - i)) & 1) === 1
</script>

<template>
  <figure class="tsr-figure">
    <div class="tsr-input">
      <textarea v-model="text" rows="1" aria-label="16-bit keys, separated by commas"></textarea>
    </div>
    <table class="tsr-table">
      <tbody>
        <tr v-for="(key, row) in keys" :key="row">
          <td>key {{ row + 1 }}</td>
          <td class="tsr-bits">
            <span v-for="(bit, i) in bitsOf(key)" :key="i" :class="{ vary: isVarying(i) }">{{ bit }}</span>
          </td>
        </tr>
        <tr class="sum">
          <td>union (OR)</td>
          <td class="tsr-bits">{{ bitsOf(union).join('') }}</td>
        </tr>
        <tr>
          <td>intersection (AND)</td>
          <td class="tsr-bits">{{ bitsOf(intersection).join('') }}</td>
        </tr>
        <tr>
          <td><strong>varying = OR ^ AND</strong></td>
          <td class="tsr-bits">
            <span v-for="(bit, i) in bitsOf(varying)" :key="i" :class="{ vary: isVarying(i) }">{{ bit }}</span>
          </td>
        </tr>
      </tbody>
    </table>
    <p class="tsr-caption">
      {{ count }} of {{ BITS }} bits vary. With 8-bit digits, a radix sort over all 16 bits makes two passes; digits that
      cover only the varying bits need {{ passes }}.
    </p>
  </figure>
</template>

<style scoped>
.vary {
  border-radius: 2px;
  background: rgba(92, 164, 248, 0.35);
  font-weight: 700;
}
.sum td {
  border-top: 2px solid var(--vp-c-divider);
}
</style>
