// SPDX-License-Identifier: BSD-2-Clause

/** Order-probe stages for an ordered, non-flat input that reaches confirmation. */
export function orderProbePlan(records: number): { kind: "sample" | "confirm"; pairs: number }[] {
  const stride = Math.max(1, Math.floor(records / 4096));
  const confirmation = { kind: "confirm" as const, pairs: Math.max(0, records - 1) };
  // At unit stride the sample would cover every pair and repeat the same work.
  return stride === 1
    ? [confirmation]
    : [{ kind: "sample", pairs: Math.floor((records - 1) / stride) }, confirmation];
}
