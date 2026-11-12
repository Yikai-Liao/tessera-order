// SPDX-License-Identifier: BSD-2-Clause

import * as Plot from "@observablehq/plot";
import { ink, label, rowName } from "./annotations.ts";
import { renderFigure } from "./figure.ts";
import { matrixLayout, type NameCell, nameWidth } from "./layout.ts";
import { type Headline, matrixHeader } from "./matrix.ts";
import { theme } from "./theme.ts";

export interface TrendCell {
  /** One entry per shared run; null leaves a gap, so every row keeps the same run positions. */
  changes: (number | null)[];
  value: string;
  major: boolean;
}
export interface TrendRow {
  name: NameCell;
  cells: (TrendCell | null)[];
  aggregate: boolean;
  /** Turns the whole row into a link, for example to a detail view in the host page. */
  href?: string;
}
export interface TrendChart {
  id?: string;
  /** Expand the plot to this host width without enlarging labels or row heights. */
  minimumWidth?: number;
  /** Shared, non-decreasing numeric run positions; omitted means equal spacing. */
  positions?: number[];
  title: NameCell;
  headline: string;
  description: string;
  columns: Headline[];
  groups: { title: string; rows: TrendRow[] }[];
  minimumSpan: number;
  reference: string;
  guide: string;
}
const color = (value: number) => (value < 0 ? ink.primary : value > 0 ? ink.radix : ink.axis);
const present = (values: (number | null)[]) => values.filter((value) => value !== null);

/** Render grouped small-multiple histories with a shared vertical scale per row. */
export function renderTrends(spec: TrendChart) {
  const rows = spec.groups.flatMap((group) => group.rows);
  if (!rows.length || !spec.columns.length)
    throw new Error("A trend chart requires rows and columns");
  if (
    !Number.isFinite(spec.minimumSpan) ||
    spec.minimumSpan <= 0 ||
    rows.some(
      (row) =>
        row.cells.length !== spec.columns.length ||
        row.cells.some(
          (cell) =>
            cell &&
            (!present(cell.changes).length ||
              present(cell.changes).some((value) => !Number.isFinite(value))),
        ),
    )
  )
    throw new Error(
      "Trend rows require matching cells, finite histories and a positive minimum span",
    );
  if (
    spec.positions &&
    (!spec.positions.length ||
      spec.positions.some(
        (x, index, xs) => !Number.isFinite(x) || (index > 0 && x < xs[index - 1]),
      ) ||
      rows.some((row) =>
        row.cells.some((cell) => cell && cell.changes.length !== spec.positions!.length),
      ))
  )
    throw new Error("Trend positions must be finite, ordered and match every history");
  const { W: width, cols } = matrixLayout(
    spec.columns.length,
    nameWidth([spec.title, ...rows.map((row) => row.name)]),
    spec.minimumWidth,
  );
  const { pad, headerRule } = theme.layout;
  const rowHeight = 46;
  let y = headerRule + 14;
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
  const bottom = y + 4,
    height = bottom + 52;
  const annotations: Plot.Markish[] = [
    label(spec.title.input, pad, 36, { strong: true }),
    label(spec.title.sub, pad, 51, { size: 11, tone: ink.muted }),
    Plot.ruleY([headerRule], { x1: pad, x2: width - pad, stroke: ink.rule }),
    ...spec.columns.flatMap((column, index) => matrixHeader(column, cols[index].left)),
  ];
  const layers: Plot.PlotOptions[] = [];
  const links: { x1: number; x2: number; y1: number; y2: number; href: string; title: string }[] =
    [];
  for (const [index, group] of groups.entries()) {
    if (index)
      annotations.push(
        Plot.ruleY([group.titleY - 18], { x1: pad, x2: width - pad, stroke: ink.rule }),
      );
    annotations.push(
      label(group.title, pad, group.titleY, { size: 10.5, caps: true, tone: ink.muted }),
    );
    for (const { row, cy } of group.placed) {
      annotations.push(...rowName(row.name, cy));
      if (row.href)
        links.push({
          x1: pad / 2,
          x2: width - pad / 2,
          y1: cy - rowHeight / 2,
          y2: cy + rowHeight / 2,
          href: row.href,
          title: [row.name.input, row.name.sub, row.name.tag].filter(Boolean).join(" · "),
        });
      const values = row.cells.flatMap((cell) => present(cell?.changes ?? []));
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
        const last = cell.changes.length - 1,
          first = cell.changes.findIndex((value) => value !== null),
          latest = cell.changes.findLastIndex((value) => value !== null),
          left = column.left + 6,
          right = column.plotR - 8;
        const xs = spec.positions ?? cell.changes.map((_, run) => run);
        const lowX = xs[0],
          highX = xs[last];
        const domain =
          lowX === highX ? (spec.positions ? [lowX - 1, highX + 1] : [-1, 0]) : [lowX, highX];
        const x: Plot.ScaleOptions = { type: "linear", domain, range: [left, right], nice: false };
        const points = cell.changes.map((value, run) => ({ value, run: xs[run] }));
        layers.push({
          x,
          y,
          marks: [
            Plot.ruleY([0], {
              x1: domain[0],
              x2: domain[1],
              stroke: ink.axis,
              strokeDasharray: "3 3",
            }),
          ],
        });
        // A null value breaks Plot's line, so a missing run stays a visible gap.
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
        if (present(cell.changes).every((value) => value === 0)) {
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
        const start = cell.changes[first]!,
          end = cell.changes[latest]!;
        layers.push({
          x,
          y,
          marks: [
            ...(latest > first
              ? [
                  Plot.dot([points[first]], {
                    x: "run",
                    y: "value",
                    r: 2.5,
                    fill: color(start),
                    stroke: ink.panel,
                    strokeWidth: 1.5,
                  }),
                ]
              : []),
            Plot.dot([points[latest]], {
              x: "run",
              y: "value",
              r: cell.major ? 5.5 : 4,
              fill: color(end),
              stroke: ink.panel,
              strokeWidth: 2,
            }),
          ],
        });
      });
    }
  }
  const legendY = bottom + 30;
  annotations.push(
    Plot.ruleY([legendY - 4], { x1: pad, x2: pad + 14, stroke: ink.axis, strokeDasharray: "3 3" }),
    label(spec.reference, pad + 20, legendY, { size: 11, tone: ink.muted }),
    label(spec.guide, width - pad, legendY, { size: 11, tone: ink.muted, anchor: "end" }),
  );
  layers.push({ marks: annotations });
  // Row links cover every mark of their row; the host page styles hover and focus.
  if (links.length)
    layers.push({
      marks: [
        Plot.rect(links, {
          x1: "x1",
          x2: "x2",
          y1: "y1",
          y2: "y2",
          href: "href",
          title: "title",
          rx: 6,
          inset: 0,
          fill: "transparent",
          className: "trend-row-link",
        }),
      ],
    });
  return renderFigure(
    { width, height, id: spec.id, title: spec.headline, description: spec.description },
    layers,
  );
}
