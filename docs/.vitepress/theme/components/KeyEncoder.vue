<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { computed, ref } from "vue";

// The encodings of `RadixKey::encode` in key.rs, on integers, f32 and a padded tuple.
type Kind = "u8" | "i8" | "f32" | "(u16, u8)";
const kind = ref<Kind>("i8");
const reverse = ref(false);
const presets: Record<Kind, string> = {
  u8: "200, 3, 255, 0, 64",
  i8: "-128, 5, -1, 0, 127, -3",
  f32: "1.5, -0.0, 0.0, -2.25, NaN, -Infinity, 3e-39, 0.5",
  "(u16, u8)": "(0, 0); (1, 0); (0, 255); (256, 5)",
};
const text = ref(presets.i8);
function setKind(next: Kind) {
  kind.value = next;
  text.value = presets[next];
}

const bits = computed(() => (kind.value === "f32" || kind.value === "(u16, u8)" ? 32 : 8));
const effectiveMask = computed(() => {
  if (kind.value === "(u16, u8)") return 0x00ffffff;
  return bits.value === 32 ? 0xffffffff : 0xff;
});

function parseRaw(part: string): number | null {
  if (kind.value === "(u16, u8)") {
    const match = /^\(\s*(\d+)\s*,\s*(\d+)\s*\)$/.exec(part);
    if (!match) return null;
    const first = Number(match[1]);
    const second = Number(match[2]);
    if (first > 65535 || second > 255) return null;
    return ((first << 8) | second) >>> 0;
  }
  const value = part === "NaN" ? NaN : Number(part);
  if (part !== "NaN" && Number.isNaN(value)) return null;
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
  const all = effectiveMask.value;
  const sign = width === 32 ? 0x80000000 : 0x80;
  let encoded: number;
  if (kind.value === "u8" || kind.value === "(u16, u8)") encoded = raw;
  else if (kind.value === "i8") encoded = raw ^ sign;
  else encoded = raw & sign ? ~raw & all : raw ^ sign;
  encoded >>>= 0;
  return reverse.value ? (~encoded & all) >>> 0 : encoded;
}

const binary = (value: number) => {
  const s = value.toString(2).padStart(bits.value, "0");
  if (kind.value === "f32") return `${s[0]} ${s.slice(1, 9)} ${s.slice(9)}`;
  if (kind.value === "(u16, u8)") return `${s.slice(0, 8)} ${s.slice(8, 24)} ${s.slice(24)}`;
  return s;
};
const inputBits = (value: number) => {
  if (kind.value === "(u16, u8)") {
    const s = value.toString(2).padStart(24, "0");
    return `${s.slice(0, 16)} · ${s.slice(16)}`;
  }
  return binary(value);
};

interface Row {
  input: string;
  raw: number;
  encoded: number;
}
const rows = computed(() => {
  const out: Row[] = [];
  const errors: string[] = [];
  const separator = kind.value === "(u16, u8)" ? ";" : ",";
  for (const part of text.value
    .split(separator)
    .map((s) => s.trim())
    .filter(Boolean)) {
    const raw = parseRaw(part);
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
      <label v-for="k in ['u8', 'i8', 'f32', '(u16, u8)'] as Kind[]" :key="k">
        <input type="radio" name="key-kind" :checked="kind === k" @change="setKind(k)" /> <code>{{ k }}</code>
      </label>
      <label><input v-model="reverse" type="checkbox" /> wrap in <code>Reverse</code></label>
      <textarea
        v-model="text"
        rows="1"
        :aria-label="kind === '(u16, u8)' ? 'Tuples, separated by semicolons' : 'Keys, separated by commas'"
      ></textarea>
    </div>
    <table class="tsr-table">
      <thead>
        <tr>
          <th>Key</th>
          <th>{{ kind === "(u16, u8)" ? "Field bits" : "Bits in memory" }}</th>
          <th>Encoded bits</th>
          <th>Encoded integer</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="row in rows.rows" :key="row.input">
          <td>{{ row.input }}</td>
          <td class="tsr-bits">{{ inputBits(row.raw) }}</td>
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
    <p v-if="kind === '(u16, u8)'" class="tsr-note">Field bits show the two scalar values (16b · 8b), not Rust tuple layout. Encoded bits pack them into u32 as 8 zero padding bits · u16 · u8; Reverse inverts only the 24 effective bits (e.g. Reverse((0, 0)) encodes to 0x00ff_ffff = 16777215).</p>
  </figure>
</template>
