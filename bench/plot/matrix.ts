// SPDX-License-Identifier: BSD-2-Clause

import type * as Plot from "@observablehq/plot";
import { ink, label, textWidth } from "./annotations.ts";
import { theme } from "./theme.ts";

/** A build's already-formatted finding; no alert or measurement protocol leaks in. */
export interface Headline {
  name: string;
  value: string;
  detail: string;
  tone: "primary" | "radix" | "muted" | "text";
}

/** Annotate a matrix's headline and row names with the shared typography. */
export function matrixHeader(headline: Headline, left: number): Plot.Markish[] {
  const nameX = left + textWidth(headline.value, theme.font.headline) + 12;
  return [
    label(headline.value, left, 50, {
      size: theme.font.headline,
      strong: true,
      tone: ink[headline.tone],
    }),
    label(headline.name, nameX, 36, { strong: true }),
    label(headline.detail, nameX, 51, { size: 11, tone: ink.muted }),
  ];
}
