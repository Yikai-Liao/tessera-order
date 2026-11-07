// SPDX-License-Identifier: BSD-2-Clause
// Twelve two-digit keys with three repeated pairs. On this input the in-place
// MSD sort reorders two of the pairs, which `radixVariants.test.ts` checks.
import type { Item } from "./radixVariants";

export const comparisonInput: Item[] = [31, 12, 27, 25, 12, 38, 21, 19, 25, 31, 30, 16].map(
  (key, id) => ({ key, id }),
);
