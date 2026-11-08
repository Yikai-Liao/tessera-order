// SPDX-License-Identifier: BSD-2-Clause

import * as Plot from "@observablehq/plot";
import { ink, label, rowName } from "./annotations.ts";
import { renderFigure } from "./figure.ts";
import { matrixLayout, type NameCell, nameWidth } from "./layout.ts";
import { type Headline, matrixHeader } from "./matrix.ts";
import { theme } from "./theme.ts";

export interface TrendCell {
  changes: number[];
  value: string;
  major: boolean;
}
export interface TrendChart {
  id?: string;
  title: NameCell;
  headline: string;
  description: string;
  columns: Headline[];
  rows: { name: NameCell; cells: (TrendCell | null)[]; aggregate: boolean }[];
  minimumSpan: number;
  reference: string;
  guide: string;
}
const color = (value: number) => (value < 0 ? ink.primary : value > 0 ? ink.radix : ink.axis);

/** Render small-multiple histories with a shared vertical scale per row. */
export function renderTrends(spec: TrendChart) {
  if (!spec.rows.length || !spec.columns.length)
    throw new Error("A trend chart requires rows and columns");
  if (
    !Number.isFinite(spec.minimumSpan) ||
    spec.minimumSpan <= 0 ||
    spec.rows.some(
      (row) =>
        row.cells.length !== spec.columns.length ||
        row.cells.some(
          (cell) =>
            cell && (!cell.changes.length || cell.changes.some((value) => !Number.isFinite(value))),
        ),
    )
  )
    throw new Error(
      "Trend rows require matching cells, finite histories and a positive minimum span",
    );
  const { W: width, cols } = matrixLayout(
    spec.columns.length,
    nameWidth([spec.title, ...spec.rows.map((row) => row.name)]),
  );
  const { pad, headerRule } = theme.layout;
  const top = headerRule + 10,
    rowHeight = 46;
  const rowTop = (index: number) => top + index * rowHeight + (index ? 12 : 0);
  const bottom = rowTop(spec.rows.length) + 4,
    height = bottom + 52;
  const annotations: Plot.Markish[] = [
    label(spec.title.input, pad, 36, { strong: true }),
    label(spec.title.sub, pad, 51, { size: 11, tone: ink.muted }),
    Plot.ruleY([headerRule], { x1: pad, x2: width - pad, stroke: ink.rule }),
    ...spec.columns.flatMap((column, index) => matrixHeader(column, cols[index].left)),
    ...(spec.rows.length > 1
      ? [Plot.ruleY([top + rowHeight + 6], { x1: pad, x2: width - pad, stroke: ink.rule })]
      : []),
  ];
  const layers: Plot.PlotOptions[] = [];
  spec.rows.forEach((row, index) => {
    const cy = rowTop(index) + rowHeight / 2;
    annotations.push(...rowName(row.name, cy));
    const values = row.cells.flatMap((cell) => cell?.changes ?? []);
    const low = Math.min(0, ...values),
      high = Math.max(0, ...values);
    const span = Math.max(high - low, spec.minimumSpan),
      mid = (low + high) / 2;
    const y: Plot.ScaleOptions = {
      type: "linear",
      domain: [mid - span / 2, mid + span / 2],
      range: [cy + 15, cy - 9],
      nice: false,
    };
    const baseline = Number(Plot.scale({ y }).apply(0));
    row.cells.forEach((cell, columnIndex) => {
      const column = cols[columnIndex];
      annotations.push(
        label(cell?.value ?? "—", column.right, cy + 4, {
          size: cell ? 12 : 11,
          tone: cell ? ink.text : ink.muted,
          strong: !!cell && (row.aggregate || cell.major),
          anchor: "end",
        }),
      );
      if (!cell) return;
      const latest = cell.changes.length - 1,
        left = column.left + 6,
        right = column.plotR - 8;
      const x: Plot.ScaleOptions = {
        type: "linear",
        domain: latest ? [0, latest] : [-1, 0],
        range: [left, right],
        nice: false,
      };
      const points = cell.changes.map((value, run) => ({ value, run }));
      layers.push({
        x,
        y,
        marks: [
          Plot.ruleY([0], {
            x1: latest ? 0 : -1,
            x2: latest || 0,
            stroke: ink.axis,
            strokeDasharray: "3 3",
          }),
        ],
      });
      const line = (stroke: string) =>
        Plot.line(points, {
          x: "run",
          y: "value",
          stroke,
          strokeWidth: 2,
          strokeLinecap: "round",
          clip: "frame",
        });
      // Plot clips one line at the baseline; no custom segment interpolation.
      if (cell.changes.every((value) => value === 0)) {
        layers.push({
          x,
          y,
          marginLeft: left,
          marginRight: width - right,
          marginTop: cy - 12,
          marginBottom: height - cy - 18,
          marks: [line(ink.axis)],
        });
      } else {
        layers.push(
          {
            x,
            y,
            marginLeft: left,
            marginRight: width - right,
            marginTop: baseline,
            marginBottom: height - cy - 18,
            marks: [line(ink.primary)],
          },
          {
            x,
            y,
            marginLeft: left,
            marginRight: width - right,
            marginTop: cy - 12,
            marginBottom: height - baseline,
            marks: [line(ink.radix)],
          },
        );
      }
      layers.push({
        x,
        y,
        marks: [
          ...(latest
            ? [
                Plot.dot([points[0]], {
                  x: "run",
                  y: "value",
                  r: 2.5,
                  fill: color(points[0].value),
                  stroke: ink.panel,
                  strokeWidth: 1.5,
                }),
              ]
            : []),
          Plot.dot([points[latest]], {
            x: "run",
            y: "value",
            r: cell.major ? 5.5 : 4,
            fill: color(points[latest].value),
            stroke: ink.panel,
            strokeWidth: 2,
          }),
        ],
      });
    });
  });
  const legendY = bottom + 30;
  annotations.push(
    Plot.ruleY([legendY - 4], { x1: pad, x2: pad + 14, stroke: ink.axis, strokeDasharray: "3 3" }),
    label(spec.reference, pad + 20, legendY, { size: 11, tone: ink.muted }),
    label(spec.guide, width - pad, legendY, { size: 11, tone: ink.muted, anchor: "end" }),
  );
  layers.push({ marks: annotations });
  return renderFigure(
    { width, height, id: spec.id, title: spec.headline, description: spec.description },
    layers,
  );
}
