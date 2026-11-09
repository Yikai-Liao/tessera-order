// SPDX-License-Identifier: BSD-2-Clause

/** Selects insertion work after the root order probe has rejected order. */
export function rootInsertion(
  records: number,
  recordBytes: number,
  tileBytes = 65536,
): "finish" | "budget" {
  return records <= 48 && records * recordBytes <= tileBytes ? "finish" : "budget";
}
