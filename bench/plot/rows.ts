// SPDX-License-Identifier: BSD-2-Clause

import * as Plot from "@observablehq/plot";
import {
  compositionDot,
  compositionLegend,
  ink,
  label,
  rowName,
  textWidth,
} from "./annotations.ts";
import { implementationAttributes, markAttributes, renderFigure } from "./figure.ts";
import { type NameCell, nameWidth } from "./layout.ts";
import { theme } from "./theme.ts";

export type Tone = "primary" | "radix" | "comparison" | "composition";
export interface Mark {
  value: number;
  tone: Tone;
  implementation?: string;
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
export type Swatch = Exclude<Tone, "composition"> | "alternate" | "hollow" | "reference" | "edge";
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

const depth: Record<Tone, number> = { comparison: 0, radix: 1, composition: 1.5, primary: 2 };
const radius = (mark: Mark) => (mark.tone === "primary" ? (mark.hollow ? 5.5 : 6.5) : 4);
const tickSize = theme.font.tiny;

/** Give the reference its own line when the tick labels would share horizontal space. */
function referenceOnSeparateLine(scale: RowChart["scale"], x: Plot.ScaleOptions, width: number) {
  const reference = scale.ticks.find((tick) => tick.value === scale.origin && tick.label);
  if (!reference) return;
  const position = Plot.scale({ x }).apply;
  // Leave room for font fallback and a visible gap between neighbouring labels.
  const halfWidth = (label: string) => textWidth(label, tickSize) * 0.55 + 2;
  const centre = position(reference.value);
  const half = halfWidth(reference.label);
  if (
    centre - half < theme.layout.pad ||
    centre + half > width - theme.layout.pad ||
    /[\r\n]/.test(reference.label) ||
    scale.ticks.some(
      (tick) =>
        tick !== reference &&
        tick.label &&
        Math.abs(position(tick.value) - centre) < half + halfWidth(tick.label),
    )
  ) {
    const available = width - 2 * theme.layout.pad;
    const inset = theme.layout.pad + Math.min(half, available / 2);
    return {
      text: reference.label.replace(/\s+/g, " "),
      x: Math.min(width - inset, Math.max(inset, centre)),
      lineWidth: (available - 4) / (tickSize * 1.1),
    };
  }
}

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
    axisY = bottom + 18;
  const x: Plot.ScaleOptions = {
    type: "log",
    base: 2,
    domain: [spec.scale.low, spec.scale.high],
    range: [left + 10, right - 10],
    clamp: spec.scale.clamp,
    nice: false,
  };
  const separateReference = referenceOnSeparateLine(spec.scale, x, width);
  const hasComposition = rows.some((row) => row.marks.some((mark) => mark.tone === "composition"));
  const directionY = axisY + (separateReference ? 32 : 16);
  const legendY = directionY + 18;
  const swatchY = legendY + (!Array.isArray(spec.legend) && hasComposition ? 16 : 0);
  const height = swatchY + 18;
  const annotations: Plot.Markish[] = [
    label(spec.title, pad, 40, { size: theme.font.title, strong: true }),
    label(spec.guide, pad, 60, { size: 11, tone: ink.muted }),
    Plot.ruleY([headerRule], { x1: pad, x2: width - pad, stroke: ink.rule }),
    label(spec.axis[0], left, directionY, { size: 11, tone: ink.muted }),
    label(spec.axis[1], right, directionY, { size: 11, tone: ink.muted, anchor: "end" }),
  ];
  if (separateReference) {
    annotations.push(
      Plot.text([separateReference], {
        x: "x",
        y: axisY + 16,
        text: "text",
        lineAnchor: "bottom",
        fontSize: tickSize,
        fill: ink.text,
        lineWidth: separateReference.lineWidth,
        textOverflow: "ellipsis-end",
      }),
    );
  }
  spec.columns.forEach((heading, index) => {
    annotations.push(
      label(heading.toUpperCase(), columns[index], layout[0].titleY, {
        size: 10.5,
        tone: ink.muted,
        caps: true,
        anchor: "end",
        // A page can restyle the columns when they show another implementation.
        ...markAttributes(() => ({ "data-comparison-heading": heading })),
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
        Plot.axisX(
          separateReference
            ? spec.scale.ticks.filter((tick) => tick.value !== spec.scale.origin)
            : spec.scale.ticks,
          {
            x: "value",
            text: "label",
            y: bottom,
            tickSize: 0,
            tickPadding: 11,
            fontSize: tickSize,
            label: null,
            fill: (tick: { value: number }) =>
              tick.value === spec.scale.origin ? ink.text : ink.muted,
          },
        ),
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

  let rowIndex = 0;
  for (const group of layout) {
    annotations.push(
      label(group.title.toUpperCase(), pad, group.titleY, {
        size: 10.5,
        tone: ink.muted,
        caps: true,
      }),
    );
    for (const { row, cy } of group.placed) {
      const rowNumber = rowIndex++;
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
            ...markAttributes(() => ({
              "data-comparison-cell": String(index),
              "data-comparison-row": String(rowNumber),
            })),
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
              ...(row.marks[0].tone === "composition"
                ? compositionDot
                : { fill: ink[row.marks[0].tone], stroke: ink.panel, strokeWidth: 2 }),
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
                fill: (mark: Mark) =>
                  mark.tone === "composition"
                    ? compositionDot.fill
                    : mark.tone === "primary"
                      ? "none"
                      : ink[mark.tone],
                stroke: (mark: Mark) =>
                  mark.tone === "composition"
                    ? compositionDot.stroke
                    : mark.tone === "primary"
                      ? "none"
                      : ink.panel,
                strokeWidth: 2,
                ...markAttributes((index) =>
                  implementationAttributes(
                    field[index] === anchor ? undefined : field[index].implementation,
                  ),
                ),
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
          ...markAttributes(() => implementationAttributes(mark.implementation)),
          ...(mark.tone === "composition" ? compositionDot : {}),
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
  if (!Array.isArray(spec.legend)) {
    annotations.push(label(spec.legend.note, pad, legendY, { size: 11, tone: ink.muted }));
  }
  if (Array.isArray(spec.legend) || hasComposition) {
    const legend: [(Swatch | "composition")[], string][] = Array.isArray(spec.legend)
      ? [...spec.legend]
      : [];
    if (hasComposition) legend.push([["composition"], compositionLegend]);
    if (clipped && Array.isArray(spec.legend)) legend.push([["edge"], "off-scale, value in table"]);
    let left = pad;
    for (const [swatches, text] of legend) {
      for (const swatch of swatches) {
        annotations.push(
          swatch === "composition"
            ? Plot.dot([[left + 4, swatchY - 4]], { r: 3.5, ...compositionDot })
            : swatch === "edge"
              ? Plot.dot([[left + 4, swatchY - 4]], {
                  symbol: "triangle",
                  rotate: 90,
                  r: 4.5,
                  fill: ink.muted,
                })
              : swatch === "reference"
                ? Plot.ruleX([left + 4], {
                    y1: swatchY - 11,
                    y2: swatchY + 3,
                    stroke: ink.axis,
                    strokeDasharray: "3 3",
                  })
                : Plot.dot([[left + 4, swatchY - 4]], {
                    r: swatch === "alternate" || swatch === "hollow" ? 3.5 : 4,
                    fill: swatch === "alternate" || swatch === "hollow" ? ink.panel : ink[swatch],
                    stroke:
                      swatch === "alternate"
                        ? ink.primary
                        : swatch === "hollow"
                          ? ink.axis
                          : "none",
                    strokeWidth: 1.5,
                  }),
        );
        left += 10;
      }
      annotations.push(label(text, left + 4, swatchY, { size: 11, tone: ink.muted }));
      left += 4 + textWidth(text, 11) + 18;
    }
  }
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
