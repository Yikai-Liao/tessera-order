// SPDX-License-Identifier: BSD-2-Clause

import { escapeXml, textWidth } from "./svg.ts";
import { theme } from "./theme.ts";

export const PAD = theme.layout.pad;
// Reserve a right-aligned value column within each plot.
export const VALUE_W = 52;

/** Lay out one label column and one plot per build configuration. */
export function matrixLayout(count: number, nameW: number) {
  const W = theme.layout.width + Math.max(0, count - 2) * 180;
  const gap = 20;
  const plotL = PAD + Math.ceil(nameW) + 24;
  const colW = (W - PAD - plotL - gap * (count - 1)) / count;
  const cols = Array.from({ length: count }, (_, i) => {
    const left = plotL + i * (colW + gap);
    return { left, right: left + colW, plotR: left + colW - VALUE_W - 6 };
  });
  return { W, plotL, cols };
}

export interface NameCell {
  input: string;
  tag: string;
  sub: string;
  strong?: boolean;
}

export const tagWidth = (tag: string) => (tag ? textWidth(tag, 10) + 12 : 0);

/** Measure the shared label column. */
export function nameWidth(names: NameCell[]) {
  // Bold text runs about 5% wider than the regular advances in textWidth.
  return Math.max(
    110,
    ...names.map((n) =>
      Math.max(
        textWidth(n.input, 13) * (n.strong ? 1.05 : 1) + (n.tag ? tagWidth(n.tag) + 6 : 0),
        textWidth(n.sub, 11),
      ),
    ),
  );
}

export type Text = (cls: string, x: number, y: number, text: string, extra?: string) => void;

/** Collect SVG fragments with escaped text and consistent coordinates. */
export function svgWriter() {
  const out: string[] = [];
  const t: Text = (cls, x, y, text, extra = "") =>
    out.push(
      `<text class="${cls}" x="${x.toFixed(1)}" y="${y.toFixed(1)}"${extra}>${escapeXml(text)}</text>`,
    );
  return { out, t };
}

/** Draw a row label with optional tag and secondary text. */
export function drawName(out: string[], t: Text, name: NameCell, cy: number) {
  t(`ink${name.strong ? " strong" : ""}`, PAD, cy - 2, name.input);
  if (name.tag) {
    const tx = PAD + textWidth(name.input, 13) + 6;
    out.push(
      `<rect class="tag" x="${tx.toFixed(1)}" y="${cy - 13}" width="${tagWidth(name.tag).toFixed(1)}" height="14" rx="7" stroke-width="1"/>`,
    );
    t("muted tiny", tx + tagWidth(name.tag) / 2, cy - 3, name.tag, ' text-anchor="middle"');
  }
  t("muted sub", PAD, cy + 13, name.sub);
}

/** Draw a configuration headline and its two-line label. */
export function drawHeader(
  t: Text,
  col: { left: number; right: number },
  hero: string,
  heroCls: string,
  name: string,
  sub: string,
  subCls = "muted sub",
) {
  t(`hero ${heroCls}`, col.left, 50, hero);
  const lx = col.left + textWidth(hero, 28) + 12;
  t("ink strong", lx, 36, name);
  t(subCls, lx, 51, sub);
}
