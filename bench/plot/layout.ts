// SPDX-License-Identifier: BSD-2-Clause

import { textWidth } from "./annotations.ts";
import { theme } from "./theme.ts";

export interface NameCell {
  input: string;
  tag: string;
  sub: string;
  strong?: boolean;
}

/** Measure a label column, including secondary text and qualifier pills. */
export function nameWidth(names: NameCell[]) {
  return Math.max(
    110,
    ...names.map((name) =>
      Math.max(
        textWidth(name.input, 13) * (name.strong ? 1.05 : 1) +
          (name.tag ? textWidth(name.tag, 10) + 18 : 0),
        textWidth(name.sub, 11),
      ),
    ),
  );
}

/** Lay out one label column and equally sized build columns. */
export function matrixLayout(count: number, namesWidth: number) {
  const width = Math.max(
    theme.layout.width + Math.max(0, count - 2) * 180,
    2 * theme.layout.pad + Math.ceil(namesWidth) + 24 + count * 140 + (count - 1) * 20,
  );
  const left = theme.layout.pad + Math.ceil(namesWidth) + 24;
  const columnWidth = (width - theme.layout.pad - left - 20 * (count - 1)) / count;
  return {
    W: width,
    cols: Array.from({ length: count }, (_, index) => {
      const x = left + index * (columnWidth + 20);
      return { left: x, right: x + columnWidth, plotR: x + columnWidth - 58 };
    }),
  };
}
