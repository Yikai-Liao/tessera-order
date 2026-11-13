// SPDX-License-Identifier: BSD-2-Clause

import * as Plot from "@observablehq/plot";
import { ink, label, rowName, textWidth } from "./annotations.ts";
import { renderFigure } from "./figure.ts";
import { type NameCell, nameWidth } from "./layout.ts";
import { theme } from "./theme.ts";

export type Tone = "primary" | "radix" | "comparison";
export interface Mark {
  value: number;
  tone: Tone;
  dy?: number;
  hollow?: boolean;
}
export interface PlotRow {
  name: NameCell;
  swarm?: boolean;
  marks: Mark[];
  span?: [number, number];
  cells: string[];
  strong?: boolean;
  /** The last cell states a lead, such as a first rank, and takes Tessera's colour. */
  lead?: boolean;
}
export type Swatch = Tone | "alternate" | "hollow" | "reference" | "edge";
export interface RowChart {
  id?: string;
  title: string;
  guide: string;
  columns: string[];
  groups: { title: string; rows: PlotRow[] }[];
  scale: {
    low: number;
    high: number;
    ticks: { value: number; label: string }[];
    origin?: number;
    clamp: boolean;
  };
  marker?: { value: number; label: string };
  legend: [Swatch[], string][] | { note: string };
  axis: [string, string];
}

const depth: Record<Tone, number> = { comparison: 0, radix: 1, primary: 2 };
const radius = (mark: Mark) => (mark.tone === "primary" ? (mark.hollow ? 5.5 : 6.5) : 4);

/** Render named rows with a shared logarithmic axis and aligned value columns.
 *
 * The template owns the table-like visual structure. Plot owns scales, axes,
 * collision placement, symbols and SVG generation. No benchmark identifiers
 * or statistical definitions are interpreted here; the input is unchanged.
 */
export function renderRows(spec: RowChart) {
  if (
    !Number.isFinite(spec.scale.low) ||
    !Number.isFinite(spec.scale.high) ||
    !(spec.scale.low > 0 && spec.scale.high > spec.scale.low)
  )
    throw new Error("A row chart requires a positive, increasing scale");
  const outside = (value: number) =>
    spec.scale.clamp && (value < spec.scale.low || value > spec.scale.high);
  const edge = (mark: Mark) => outside(mark.value);
  const groups = spec.groups.filter((group) => group.rows.length);
  const rows = groups.flatMap((group) => group.rows);
  if (!rows.length || !spec.columns.length)
    throw new Error("A row chart requires rows and value columns");
  for (const row of rows) {
    const values = [...row.marks.map((mark) => mark.value), ...(row.span ?? [])];
    if (
      row.cells.length !== spec.columns.length ||
      values.some((value) => !Number.isFinite(value) || value <= 0)
    )
      throw new Error("Rows require matching value cells and positive finite plot values");
  }

  const { pad, headerRule } = theme.layout;
  const top = headerRule + 14;
  const rowHeight = rows.some((row) => row.name.sub) ? theme.layout.row : theme.layout.singleRow;
  const columnWidths = spec.columns.map((heading, index) =>
    Math.max(
      textWidth(heading, 11) * 1.3,
      ...rows.map((row) => textWidth(row.cells[index], 12) * 1.05),
    ),
  );
  const valuesWidth =
    columnWidths.reduce((sum, width) => sum + width, 0) + (columnWidths.length - 1) * 20;
  const namesWidth = Math.ceil(nameWidth(rows.map((row) => row.name)));
  const width = Math.max(theme.layout.width, 2 * pad + namesWidth + 28 + 160 + 24 + valuesWidth);
  const columns = columnWidths.map((_, index) =>
    columnWidths.slice(index + 1).reduce((right, value) => right - value - 20, width - pad),
  );
  const left = pad + namesWidth + 28;
  const right = columns[0] - columnWidths[0] - 24;

  let y = top;
  const layout = groups.map((group, index) => {
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
    axisY = bottom + 18,
    legendY = axisY + 34,
    height = legendY + 18;
  const x: Plot.ScaleOptions = {
    type: "log",
    base: 2,
    domain: [spec.scale.low, spec.scale.high],
    range: [left + 10, right - 10],
    clamp: spec.scale.clamp,
    nice: false,
  };
  const annotations: Plot.Markish[] = [
    label(spec.title, pad, 40, { size: theme.font.title, strong: true }),
    label(spec.guide, pad, 60, { size: 11, tone: ink.muted }),
    Plot.ruleY([headerRule], { x1: pad, x2: width - pad, stroke: ink.rule }),
    label(spec.axis[0], left, axisY + 16, { size: 11, tone: ink.muted }),
    label(spec.axis[1], right, axisY + 16, { size: 11, tone: ink.muted, anchor: "end" }),
  ];
  spec.columns.forEach((heading, index) => {
    annotations.push(
      label(heading.toUpperCase(), columns[index], layout[0].titleY, {
        size: 10.5,
        tone: ink.muted,
        caps: true,
        anchor: "end",
      }),
    );
  });
  const layers: Plot.PlotOptions[] = [
    {
      x,
      marks: [
        Plot.ruleX(spec.scale.ticks, {
          x: "value",
          y1: top,
          y2: bottom,
          stroke: (tick: { value: number }) =>
            tick.value === spec.scale.origin ? ink.axis : ink.rule,
        }),
        Plot.axisX(spec.scale.ticks, {
          x: "value",
          text: "label",
          y: bottom,
          tickSize: 0,
          tickPadding: 11,
          fontSize: 10,
          label: null,
          fill: (tick: { value: number }) =>
            tick.value === spec.scale.origin ? ink.text : ink.muted,
        }),
        ...(spec.marker
          ? [
              Plot.ruleX([spec.marker.value], {
                y1: top - 2,
                y2: bottom,
                stroke: ink.axis,
                strokeDasharray: "3 3",
              }),
              Plot.text([{ value: spec.marker.value, text: spec.marker.label }], {
                x: "value",
                y: top - 4,
                text: "text",
                lineAnchor: "bottom",
                fontSize: 10,
                fill: ink.muted,
              }),
            ]
          : []),
      ],
    },
  ];

  for (const group of layout) {
    annotations.push(
      label(group.title.toUpperCase(), pad, group.titleY, {
        size: 10.5,
        tone: ink.muted,
        caps: true,
      }),
    );
    for (const { row, cy } of group.placed) {
      annotations.push(...rowName(row.name, cy));
      row.cells.forEach((cell, index) => {
        annotations.push(
          label(cell, columns[index], cy + 4, {
            size: 12,
            tone:
              row.lead && index === row.cells.length - 1
                ? ink.primary
                : row.strong
                  ? ink.text
                  : ink.muted,
            strong: !!row.strong && index === row.cells.length - 1,
            anchor: "end",
          }),
        );
      });
      if (row.span) {
        const inset = (value: number) => {
          const ring = row.marks.find((mark) => mark.hollow && mark.value === value);
          return ring ? radius(ring) : 0;
        };
        layers.push({
          x,
          marks: [
            Plot.ruleY([cy], {
              x1: row.span[0],
              x2: row.span[1],
              insetLeft: inset(row.span[0]),
              insetRight: inset(row.span[1]),
              stroke: ink[row.marks[0].tone],
              strokeWidth: 2,
              strokeLinecap: "round",
            }),
            // A range can cross an axis limit while its median stays inside.
            Plot.dot(row.span.filter(outside), {
              x: (value: number) => value,
              y: cy,
              symbol: "triangle",
              rotate: (value: number) => (value < spec.scale.low ? 270 : 90),
              r: 4.5,
              fill: ink[row.marks[0].tone],
              stroke: ink.panel,
              strokeWidth: 2,
            }),
          ],
        });
      }
      const ordered = [...row.marks].sort(
        (a, b) => depth[a.tone] + Number(!!a.hollow) / 2 - depth[b.tone] - Number(!!b.hollow) / 2,
      );
      const primary = ordered.filter((mark) => mark.tone === "primary");
      const competitors = ordered.filter((mark) => mark.tone !== "primary");
      if (row.swarm && competitors.length) {
        // The highlighted filled dot anchors the field; Plot places competitors.
        const anchor = primary.find((mark) => !mark.hollow);
        const field = [...(anchor ? [anchor] : []), ...competitors];
        layers.push({
          x,
          marginTop: cy,
          marginBottom: height - cy,
          marks: [
            Plot.dot(
              field,
              Plot.dodgeY("middle", {
                x: "value",
                r: (mark: Mark) => (edge(mark) ? 4.5 : radius(mark)),
                symbol: (mark: Mark) => (edge(mark) ? "triangle" : "circle"),
                rotate: (mark: Mark) => (edge(mark) ? (mark.value < spec.scale.low ? 270 : 90) : 0),
                fill: (mark: Mark) => (mark.tone === "primary" ? "none" : ink[mark.tone]),
                stroke: (mark: Mark) => (mark.tone === "primary" ? "none" : ink.panel),
                strokeWidth: 2,
              }),
            ),
          ],
        });
      }
      for (const mark of row.swarm ? primary : ordered) {
        const dot = {
          x: "value",
          y: cy + (mark.dy ?? 0),
          r: radius(mark),
          fill: ink[mark.tone],
          stroke: ink.panel,
          strokeWidth: 2,
        };
        layers.push({
          x,
          marks: edge(mark)
            ? [
                Plot.dot([mark], {
                  ...dot,
                  symbol: "triangle",
                  rotate: mark.value < spec.scale.low ? 270 : 90,
                  r: 4.5,
                }),
              ]
            : mark.hollow
              ? [
                  Plot.dot([mark], { ...dot, fill: "none", strokeWidth: 4 }),
                  Plot.dot([mark], {
                    ...dot,
                    fill: "none",
                    stroke: ink[mark.tone],
                    strokeWidth: 2,
                  }),
                ]
              : [Plot.dot([mark], dot)],
        });
      }
    }
  }
  // An off-scale mark is a triangle at the axis end, so a chart that draws one explains it.
  const clipped = rows.some(
    (row) => row.marks.some((mark) => outside(mark.value)) || (row.span ?? []).some(outside),
  );
  if (Array.isArray(spec.legend)) {
    const legend: [Swatch[], string][] = clipped
      ? [...spec.legend, [["edge"], "off-scale, value at right"]]
      : spec.legend;
    let left = pad;
    for (const [swatches, text] of legend) {
      for (const swatch of swatches) {
        annotations.push(
          swatch === "edge"
            ? Plot.dot([[left + 4, legendY - 4]], {
                symbol: "triangle",
                rotate: 90,
                r: 4.5,
                fill: ink.muted,
              })
            : swatch === "reference"
              ? Plot.ruleX([left + 4], {
                  y1: legendY - 11,
                  y2: legendY + 3,
                  stroke: ink.axis,
                  strokeDasharray: "3 3",
                })
              : Plot.dot([[left + 4, legendY - 4]], {
                  r: swatch === "alternate" || swatch === "hollow" ? 3.5 : 4,
                  fill: swatch === "alternate" || swatch === "hollow" ? ink.panel : ink[swatch],
                  stroke:
                    swatch === "alternate" ? ink.primary : swatch === "hollow" ? ink.axis : "none",
                  strokeWidth: 1.5,
                }),
        );
        left += 10;
      }
      annotations.push(label(text, left + 4, legendY, { size: 11, tone: ink.muted }));
      left += 4 + textWidth(text, 11) + 18;
    }
  } else annotations.push(label(spec.legend.note, pad, legendY, { size: 11, tone: ink.muted }));
  layers.push({ marks: annotations });
  return `<!-- SPDX-License-Identifier: BSD-2-Clause -->\n${renderFigure(
    {
      width,
      height,
      id: spec.id,
      title: spec.title,
      description: `${spec.guide}. ${spec.columns.join(", ")}: ${rows.map((row) => `${row.name.input} ${row.cells.join(", ")}`).join("; ")}`,
    },
    layers,
  )}\n`;
}
