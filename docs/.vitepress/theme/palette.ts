// SPDX-License-Identifier: BSD-2-Clause
// Bucket colours are the logo's ramp, which already runs in key order: low
// digits are teal, high digits are coral. Every coloured cell also carries its
// key as text, so colour is never the only cue.
const ramp = [
  "#2DD4BF",
  "#37C0F1",
  "#5CA4F8",
  "#7892F8",
  "#A885F8",
  "#DB7BF9",
  "#EF82B2",
  "#F98F54",
];

export function bucketColor(bucket: number, buckets: number): string {
  if (buckets <= 1) return ramp[0];
  return ramp[Math.round((bucket * (ramp.length - 1)) / (buckets - 1))];
}

/** Original positions as letters, so stability is visible: equal keys keep a, b, c order. */
export function recordLabel(id: number): string {
  return id < 26 ? String.fromCharCode(97 + id) : `#${id}`;
}
