<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { computed, ref } from "vue";

// Mirrors `RadixKey::encode` in rust/tessera-order/src/engine/key.rs.
type Kind = "u8" | "i8" | "f32" | "(u16, u8)";
const kind = ref<Kind>("i8");
const reverse = ref(false);
const presets: Record<Kind, string> = {
  i8: "-128, -3, -1, 0, 5, 127",
  u8: "0, 3, 64, 200, 255",
  f32: "-Infinity, -2.25, -0.0, 0.0, 3e-39, 1.5, NaN",
  "(u16, u8)": "(0, 0); (0, 255); (1, 0); (256, 5)",
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

function groupedChars(raw: number, encoded: number) {
  const r = raw.toString(2).padStart(bits.value, "0");
  const e = encoded.toString(2).padStart(bits.value, "0");
  const breaks =
    kind.value === "f32"
      ? new Set([1, 9])
      : kind.value === "(u16, u8)"
        ? new Set([8, 24])
        : new Set<number>();
  return e.split("").map((ch, i) => ({
    ch,
    flipped: ch !== r[i],
    pad: kind.value === "(u16, u8)" && i < 8,
    gap: breaks.has(i),
  }));
}

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
    <div class="ke-bar">
      <div class="tsr-seg" role="tablist" aria-label="Key type">
        <button
          v-for="k in (['i8', 'u8', 'f32', '(u16, u8)'] as Kind[])"
          :key="k"
          type="button"
          role="tab"
          :aria-selected="kind === k"
          :class="{ active: kind === k }"
          @click="setKind(k)"
        >
          <code>{{ k }}</code>
        </button>
      </div>
      <label class="ke-toggle">
        <input v-model="reverse" type="checkbox" />
        Wrap in <code>Reverse</code>
      </label>
    </div>

    <div class="tsr-input">
      <textarea
        v-model="text"
        rows="1"
        :aria-label="kind === '(u16, u8)' ? 'Tuples, separated by semicolons' : 'Keys, separated by commas'"
      ></textarea>
    </div>

    <table class="tsr-table ke-table">
      <thead>
        <tr>
          <th>Key</th>
          <th>Encoded bits <small>(highlighted = flipped from raw bits)</small></th>
          <th>Unsigned value</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="row in sorted" :key="row.input">
          <td><strong>{{ row.input }}</strong></td>
          <td class="tsr-bits">
            <span
              v-for="(bit, i) in groupedChars(row.raw, row.encoded)"
              :key="i"
              :class="{ flipped: bit.flipped, pad: bit.pad, gap: bit.gap }"
            >{{ bit.ch }}</span>
          </td>
          <td>{{ row.encoded.toLocaleString('en-US') }}</td>
        </tr>
      </tbody>
    </table>

    <p class="tsr-caption">
      Rows are ordered by their encoded unsigned integer:
      <code>{{ sorted.map((r) => r.input).join('  <  ') }}</code>
    </p>
    <p v-if="rows.errors.length" class="tsr-note">Not a valid <code>{{ kind }}</code>: {{ rows.errors.join(', ') }}</p>
  </figure>
</template>

<style scoped>
.ke-bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 10px 14px;
  margin-bottom: 10px;
}
.ke-toggle {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  cursor: pointer;
}
.ke-table th small {
  font-weight: 400;
  color: var(--vp-c-text-3);
}
.flipped {
  border-radius: 2px;
  background: var(--vp-c-brand-soft);
  color: var(--vp-c-brand-1);
  font-weight: 700;
}
.pad {
  opacity: 0.35;
}
.gap {
  margin-left: 6px;
}
</style>
