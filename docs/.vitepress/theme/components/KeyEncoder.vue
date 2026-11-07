<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { computed, ref } from "vue";

// The encodings of `RadixKey::encode` in key.rs, on 8-bit integers and f32.
type Kind = "u8" | "i8" | "f32";
const kind = ref<Kind>("i8");
const reverse = ref(false);
const presets: Record<Kind, string> = {
  u8: "200, 3, 255, 0, 64",
  i8: "-128, 5, -1, 0, 127, -3",
  f32: "1.5, -0.0, 0.0, -2.25, NaN, -Infinity, 3e-39, 0.5",
};
const text = ref(presets.i8);
function setKind(next: Kind) {
  kind.value = next;
  text.value = presets[next];
}

const bits = computed(() => (kind.value === "f32" ? 32 : 8));

function rawBits(value: number): number | null {
  if (kind.value === "f32") {
    const view = new DataView(new ArrayBuffer(4));
    view.setFloat32(0, value);
    return view.getUint32(0);
  }
  if (!Number.isInteger(value)) return null;
  if (kind.value === "u8") return value >= 0 && value <= 255 ? value : null;
  return value >= -128 && value <= 127 ? value & 0xff : null;
}

function encode(raw: number): number {
  const width = bits.value;
  const all = width === 32 ? 0xffffffff : 0xff;
  const sign = width === 32 ? 0x80000000 : 0x80;
  let encoded: number;
  if (kind.value === "u8") encoded = raw;
  else if (kind.value === "i8") encoded = raw ^ sign;
  else encoded = raw & sign ? ~raw & all : raw ^ sign;
  encoded >>>= 0;
  return reverse.value ? (~encoded & all) >>> 0 : encoded;
}

const binary = (value: number) => {
  const s = value.toString(2).padStart(bits.value, "0");
  return kind.value === "f32" ? `${s[0]} ${s.slice(1, 9)} ${s.slice(9)}` : s;
};

interface Row {
  input: string;
  raw: number;
  encoded: number;
}
const rows = computed(() => {
  const out: Row[] = [];
  const errors: string[] = [];
  for (const part of text.value
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean)) {
    const value = Number(part);
    const raw =
      part === "NaN" || !Number.isNaN(value) ? rawBits(part === "NaN" ? NaN : value) : null;
    if (raw === null) errors.push(part);
    else out.push({ input: part, raw, encoded: encode(raw) });
  }
  return { rows: out, errors };
});
const sorted = computed(() => [...rows.value.rows].sort((a, b) => a.encoded - b.encoded));
</script>

<template>
  <figure class="tsr-figure">
    <div class="tsr-input">
      <label v-for="k in ['u8', 'i8', 'f32'] as Kind[]" :key="k">
        <input type="radio" name="key-kind" :checked="kind === k" @change="setKind(k)" /> <code>{{ k }}</code>
      </label>
      <label><input v-model="reverse" type="checkbox" /> wrap in <code>Reverse</code></label>
      <textarea v-model="text" rows="1" aria-label="Keys, separated by commas"></textarea>
    </div>
    <table class="tsr-table">
      <thead>
        <tr>
          <th>Key</th>
          <th>Bits in memory</th>
          <th>Encoded bits</th>
          <th>Encoded integer</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="row in rows.rows" :key="row.input">
          <td>{{ row.input }}</td>
          <td class="tsr-bits">{{ binary(row.raw) }}</td>
          <td class="tsr-bits">{{ binary(row.encoded) }}</td>
          <td>{{ row.encoded }}</td>
        </tr>
      </tbody>
    </table>
    <p class="tsr-caption">
      Sorted by encoded integer:
      <code>{{ sorted.map((r) => r.input).join('  <  ') }}</code>
    </p>
    <p v-if="rows.errors.length" class="tsr-note">Not a valid <code>{{ kind }}</code>: {{ rows.errors.join(', ') }}</p>
    <p v-if="kind === 'f32'" class="tsr-note">f32 bits are shown as sign · exponent · mantissa.</p>
  </figure>
</template>
