// SPDX-License-Identifier: BSD-2-Clause

/** Complete order-probe stages for an ordered, non-flat input. */
export function orderProbePlan(records: number): { kind: "sample" | "confirm"; pairs: number }[] {
  const stride = Math.max(1, Math.floor(records / 4096));
  const confirmation = { kind: "confirm" as const, pairs: Math.max(0, records - 1) };
  // Both routes share confirmation: reject a mixed prefix, then scan the
  // unchecked suffix in one direction. Larger strides add sampling first.
  // The confirm stage counts the complete scan, not a separate backend dispatch.
  return stride === 1
    ? [confirmation]
    : [{ kind: "sample", pairs: Math.floor((records - 1) / stride) }, confirmation];
}
