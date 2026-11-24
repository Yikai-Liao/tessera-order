// SPDX-License-Identifier: BSD-2-Clause

import { implementation, isComposition } from "../catalog.ts";
import type { Tone } from "../plot/rows.ts";

/** Figure tone for a catalog implementation. */
export const toneOf = (variant: string): Tone =>
  variant === "tessera"
    ? "primary"
    : isComposition(variant)
      ? "composition"
      : implementation(variant).family === "radix"
        ? "radix"
        : "comparison";
