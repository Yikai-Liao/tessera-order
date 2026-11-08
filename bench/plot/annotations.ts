// SPDX-License-Identifier: BSD-2-Clause

import * as Plot from "@observablehq/plot";
import measure from "string-pixel-width";
import type { NameCell } from "./layout.ts";
import { theme } from "./theme.ts";

/** Semantic ink shared by marks, labels and the light/dark SVG stylesheet. */
export const ink = {
  primary: theme.palette.blue,
  radix: theme.palette.orange,
  comparison: theme.palette.grey,
  text: "var(--ink)",
  muted: "var(--muted)",
  rule: "var(--rule)",
  axis: "var(--axis)",
  panel: "var(--panel)",
} as const;

/** Measure Inter labels with the library's font metrics, without a canvas. */
export const textWidth = (text: string, size: number) => measure(text, { font: "inter", size });

export interface LabelOptions {
  size?: number;
  tone?: string;
  strong?: boolean;
  anchor?: "start" | "middle" | "end";
  caps?: boolean;
}

/** Place a baseline-aligned annotation using the figure's typography. */
export function label(text: string, x: number, y: number, options: LabelOptions = {}) {
  return Plot.text([{ text, x, y }], {
    x: "x",
    y: "y",
    text: "text",
    textAnchor: options.anchor ?? "start",
    lineAnchor: "bottom",
    dy: 0,
    fontSize: options.size ?? theme.font.body,
    fontWeight: options.strong || options.caps ? 600 : 400,
    fill: options.tone ?? ink.text,
    ariaLabel: options.caps ? "group heading" : "annotation",
  });
}

/** Annotate a named row, including a compact tag when the case has a qualifier. */
export function rowName(name: NameCell, cy: number): Plot.Markish[] {
  const { pad } = theme.layout;
  const labels: Plot.Markish[] = [
    label(name.input, pad, cy + (name.sub ? -2 : 4), { strong: name.strong }),
    ...(name.sub ? [label(name.sub, pad, cy + 13, { size: 11, tone: ink.muted })] : []),
  ];
  if (name.tag) {
    const left = pad + textWidth(name.input, 13) + 6,
      width = textWidth(name.tag, 10) + 12;
    labels.push(
      Plot.rect([{ x1: left, x2: left + width, y1: cy - 13, y2: cy + 1 }], {
        x1: "x1",
        x2: "x2",
        y1: "y1",
        y2: "y2",
        rx: 7,
        fill: "none",
        stroke: "var(--tag)",
        inset: 0,
      }),
      label(name.tag, left + width / 2, cy - 3, { size: 10, tone: ink.muted, anchor: "middle" }),
    );
  }
  return labels;
}
