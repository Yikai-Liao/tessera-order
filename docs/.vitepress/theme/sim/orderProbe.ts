// SPDX-License-Identifier: BSD-2-Clause

/** Logical pair checks in full confirmation, excluding backend look-ahead. */
export function orderConfirmationPlan(
  records: number,
  direction: "ascending" | "descending",
  leadingTies = 0,
): { direction: "ascending" | "descending"; pairs: number }[] {
  const pairs = Math.max(0, records - 1);
  return direction === "ascending"
    ? [{ direction, pairs }]
    : [
        { direction: "ascending", pairs: Math.min(pairs, leadingTies + 1) },
        { direction, pairs },
      ];
}

/** Complete order-probe stages for an ascending, non-flat input. */
export function orderProbePlan(records: number): { kind: "sample" | "confirm"; pairs: number }[] {
  const stride = Math.max(1, Math.floor(records / 4096));
  const confirmation = { kind: "confirm" as const, pairs: Math.max(0, records - 1) };
  // Unit stride rejects a minimal mixed prefix, then scans one direction.
  // Larger strides sample first; both paths count a complete confirmation here.
  // The confirm stage counts the complete scan, not a separate backend dispatch.
  return stride === 1
    ? [confirmation]
    : [{ kind: "sample", pairs: Math.floor((records - 1) / stride) }, confirmation];
}
