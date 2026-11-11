// SPDX-License-Identifier: BSD-2-Clause

import * as Plot from "@observablehq/plot";
import { ink, label, rowName } from "./annotations.ts";
import { renderFigure } from "./figure.ts";
import { matrixLayout, type NameCell, nameWidth } from "./layout.ts";
import { type Headline, matrixHeader } from "./matrix.ts";
import { theme } from "./theme.ts";

export interface ChangeCell {
  change: number | null;
  value: string;
  status: "slower" | "faster" | "within" | "missing";
  major: boolean;
}
export interface ChangeChart {
  id?: string;
  title: NameCell;
  description: string;
  headline: string;
  columns: Headline[];
  groups: { title: string; rows: { name: NameCell; cells: (ChangeCell | null)[] }[] }[];
  folded?: string;
  majorLabel: string;
}

const percent = new Intl.NumberFormat("en-US", {
  style: "percent",
  maximumFractionDigits: 2,
  signDisplay: "exceptZero",
});
const tickLabel = (value: number) => (value === 0 ? "0" : percent.format(value).replace("-", "−"));

/** Draw comparable changes on one linear scale across build columns. */
export function renderChanges(spec: ChangeChart) {
  const rows = spec.groups.flatMap((group) => group.rows);
  if (!rows.length || !spec.columns.length)
    throw new Error("A change chart requires rows and columns");
  if (
    rows.some(
      (row) =>
        row.cells.length !== spec.columns.length ||
        row.cells.some((cell) => cell?.change != null && !Number.isFinite(cell.change)),
    )
  )
    throw new Error("Change rows require matching cells and finite changes");
  const { W: width, cols } = matrixLayout(
    spec.columns.length,
    nameWidth([spec.title, ...rows.map((row) => row.name)]),
  );
  const { pad, headerRule, row: rowHeight } = theme.layout;
  const top = headerRule + 14;
  let y = top;
  const groups = spec.groups.map((group, index) => {
    y += index ? 10 : 0;
    const titleY = y + 12;
    y += 22;
    const placed = group.rows.map((row) => {
      const cy = y + rowHeight / 2;
      y += rowHeight;
      return { row, cy };
    });
    return { title: group.title, titleY, placed };
  });
  if (spec.folded) y += 22;
  const bottom = y + 4,
    height = bottom + 52;
  const values = rows.flatMap((row) =>
    row.cells.flatMap((cell) => (cell?.change == null ? [] : [cell.change])),
  );
  const low = Math.min(0, ...values),
    high = Math.max(0, ...values);
  const annotations: Plot.Markish[] = [
    label(spec.title.input, pad, 36, { strong: true }),
    label(spec.title.sub, pad, 51, { size: 11, tone: ink.muted }),
    Plot.ruleY([headerRule], { x1: pad, x2: width - pad, stroke: ink.rule }),
    ...spec.columns.flatMap((column, index) => matrixHeader(column, cols[index].left)),
  ];
  for (const group of groups) {
    annotations.push(
      label(group.title, pad, group.titleY, { size: 10.5, caps: true, tone: ink.muted }),
    );
    for (const { row, cy } of group.placed) {
      annotations.push(...rowName(row.name, cy));
      row.cells.forEach((cell, index) => {
        if (cell)
          annotations.push(
            label(cell.value, cols[index].right, cy + 4, {
              size: cell.change === null ? 11 : 12,
              tone: cell.status === "within" || cell.change === null ? ink.muted : ink.text,
              strong: cell.change !== null && cell.status !== "within",
              anchor: "end",
            }),
          );
      });
    }
  }
  if (spec.folded)
    annotations.push(label(spec.folded, pad, bottom - 10, { size: 11, tone: ink.muted }));
  const layers: Plot.PlotOptions[] = cols.map((column, index) => {
    const cells = groups.flatMap((group) =>
      group.placed.flatMap(({ row, cy }) => {
        const cell = row.cells[index];
        return cell?.change == null ? [] : [{ ...cell, cy }];
      }),
    );
    const alert = cells.filter((cell) => cell.status !== "within");
    const quiet = cells.filter((cell) => cell.status === "within");
    // Plot treats tick count as a hint; reserve room for its rounded intervals.
    const ticks = Math.max(1, (column.plotR - column.left - 16) / 64);
    return {
      x: {
        type: "linear",
        domain: [low, high === low ? low + 1e-9 : high],
        range: [column.left + 8, column.plotR - 8],
        nice: false,
      },
      marks: [
        Plot.gridX({ ticks, y1: top, y2: bottom, stroke: ink.rule, strokeOpacity: 1 }),
        Plot.ruleX([0], { y1: top, y2: bottom, stroke: ink.axis }),
        Plot.axisX({
          ticks,
          y: bottom,
          tickSize: 0,
          tickPadding: 9,
          fontSize: 10,
          tickFormat: tickLabel,
          label: null,
          color: ink.muted,
        }),
        Plot.ruleY(alert, {
          y: "cy",
          x1: 0,
          x2: "change",
          stroke: (cell: ChangeCell) => (cell.status === "slower" ? ink.radix : ink.primary),
          strokeWidth: 2,
          strokeLinecap: "round",
        }),
        Plot.dot(quiet, {
          x: "change",
          y: "cy",
          r: 3.5,
          fill: ink.panel,
          stroke: ink.axis,
          strokeWidth: 1.5,
        }),
        Plot.dot(alert, {
          x: "change",
          y: "cy",
          r: (cell: ChangeCell) => (cell.major ? 5.5 : 4),
          fill: (cell: ChangeCell) => (cell.status === "slower" ? ink.radix : ink.primary),
          stroke: ink.panel,
          strokeWidth: 2,
        }),
      ],
    };
  });
  const legendY = bottom + 38;
  annotations.push(
    Plot.dot([[pad + 4, legendY - 4]], { r: 4, fill: ink.radix }),
    Plot.dot([[pad + 14, legendY - 4]], { r: 4, fill: ink.primary }),
    label("alert", pad + 24, legendY, { size: 11, tone: ink.muted }),
    Plot.dot([[pad + 66, legendY - 4]], {
      r: 3.5,
      fill: ink.panel,
      stroke: ink.axis,
      strokeWidth: 1.5,
    }),
    label("no alert", pad + 76, legendY, { size: 11, tone: ink.muted }),
    Plot.dot([[pad + 158, legendY - 4]], { r: 5.5, fill: ink.text }),
    label(spec.majorLabel, pad + 169, legendY, { size: 11, tone: ink.muted }),
    label("more time →", width - pad, legendY, { size: 11, tone: ink.radix, anchor: "end" }),
    label("← less time", width - pad - 85, legendY, { size: 11, tone: ink.primary, anchor: "end" }),
  );
  layers.push({ marks: annotations });
  return renderFigure(
    { width, height, id: spec.id, title: spec.headline, description: spec.description },
    layers,
  );
}
