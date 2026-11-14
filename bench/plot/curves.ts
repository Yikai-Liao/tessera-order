// SPDX-License-Identifier: BSD-2-Clause

import * as Plot from "@observablehq/plot";
import { ink, label, textWidth } from "./annotations.ts";
import { renderFigure } from "./figure.ts";
import type { Tone } from "./rows.ts";
import { theme } from "./theme.ts";

export interface Axis {
  low: number;
  high: number;
  ticks: { value: number; label: string }[];
  /** Names the quantity at the axis end, such as "records". */
  label: string;
}
export interface Curve {
  name: string;
  tone: Tone;
  strong?: boolean;
  /** Increasing x; a y below the axis is drawn as a triangle on its edge. */
  points: [number, number][];
}
export interface CurveChart {
  id?: string;
  title: string;
  guide: string;
  x: Axis;
  y: Axis;
  curves: Curve[];
  /** Dashed or dotted guides that are not measurements; the legend names them. */
  references: { dotted?: boolean; points: [number, number][] }[];
  legend: [CurveSwatch[], string][];
  /** A sentence under the legend, for sorts the axes cannot show. */
  note?: string;
}

/** A legend key: a sort kind's line, a reference's dash pattern, or the off-axis triangle. */
export type CurveSwatch = Tone | "dashed" | "dotted" | "edge";

const LABEL_GAP = 14;

/** Spread end labels vertically by at least the label height, keeping their order. */
function dodge(targets: number[], low: number, high: number) {
  const order = targets.map((_, index) => index).sort((a, b) => targets[a] - targets[b]);
  const placed = new Array<number>(targets.length);
  let previous = -Infinity;
  for (const index of order) {
    placed[index] = Math.max(targets[index], previous + LABEL_GAP);
    previous = placed[index];
  }
  // Shift the stack back inside the plot if it ran past the bottom.
  const overflow = Math.max(0, previous - high);
  for (const index of order) placed[index] = Math.max(low, placed[index] - overflow);
  return placed;
}

/** Render measured curves on shared logarithmic axes, each named at its right end.
 *
 * A power of the x quantity is a straight line on these axes, so a reference line
 * with a known slope lets a reader read growth orders directly.
 */
export function renderCurves(spec: CurveChart) {
  for (const axis of [spec.x, spec.y])
    if (!(axis.low > 0 && axis.high > axis.low && Number.isFinite(axis.high)))
      throw new Error("A curve chart requires positive, increasing axes");
  if (!spec.curves.length) throw new Error("A curve chart requires curves");
  for (const curve of [...spec.curves, ...spec.references])
    if (
      !curve.points.length ||
      curve.points.some(
        ([x, y], index, points) =>
          !(x > 0 && Number.isFinite(x)) ||
          !(y >= 0 && Number.isFinite(y)) ||
          (index > 0 && x <= points[index - 1][0]),
      )
    )
      throw new Error("Curves require increasing positive x and finite non-negative y");

  const { pad, headerRule } = theme.layout;
  const width = theme.layout.width;
  const yTicksWidth = Math.max(...spec.y.ticks.map((tick) => textWidth(tick.label, 10))) + 12;
  const left = pad + yTicksWidth;
  const top = headerRule + 24;
  const bottom = top + 300;
  const scale = (axis: Axis, from: number, to: number) => (value: number) =>
    from +
    ((Math.log(value) - Math.log(axis.low)) / (Math.log(axis.high) - Math.log(axis.low))) *
      (to - from);
  const toY = scale(spec.y, bottom, top);
  const py = (value: number) => (value < spec.y.low ? bottom : Math.max(top, toY(value)));
  const below = (value: number) => value < spec.y.low;
  const clipped = spec.curves.some((curve) => curve.points.some(([, y]) => below(y)));

  // Lines ending at the same value share one end label, in drawing order.
  const rows: [number, Curve[]][] = [];
  for (const curve of spec.curves.toSorted((a, b) => a.points.at(-1)![1] - b.points.at(-1)![1])) {
    const y = Math.round(py(curve.points.at(-1)![1]));
    const row = rows.find(([at]) => at === y);
    if (row) row[1].push(curve);
    else rows.push([y, [curve]]);
  }
  const separator = ", ";
  const rowWidth = (curves: Curve[]) =>
    textWidth(curves.map((curve) => curve.name).join(separator), 12) +
    (curves.some((curve) => curve.tone === "primary") ? 4 : 0);
  const labelsWidth = Math.max(...rows.map(([, curves]) => rowWidth(curves))) + 28;
  const right = width - pad - labelsWidth;
  const px = scale(spec.x, left, right);

  // The note wraps at the panel width, below the legend.
  const noteLines: string[] = [];
  for (const word of spec.note?.split(" ") ?? []) {
    const line = noteLines.at(-1);
    if (line !== undefined && textWidth(`${line} ${word}`, 11) <= width - 2 * pad)
      noteLines[noteLines.length - 1] = `${line} ${word}`;
    else noteLines.push(word);
  }
  const axisY = bottom + 30;
  const legendY = axisY + 34;
  const height = legendY + 18 + noteLines.length * 18;

  const marks: Plot.Markish[] = [
    label(spec.title, pad, 40, { size: theme.font.title, strong: true }),
    label(spec.guide, pad, 60, { size: 11, tone: ink.muted }),
    Plot.ruleY([headerRule], { x1: pad, x2: width - pad, stroke: ink.rule }),
    Plot.ruleY(
      spec.y.ticks.map((tick) => py(tick.value)),
      { x1: left, x2: right, stroke: ink.rule },
    ),
    Plot.ruleX(
      spec.x.ticks.map((tick) => px(tick.value)),
      { y1: top, y2: bottom, stroke: ink.rule },
    ),
    ...spec.y.ticks.map((tick) =>
      label(tick.label, left - 8, py(tick.value) + 4, { size: 10, tone: ink.muted, anchor: "end" }),
    ),
    ...spec.x.ticks.map((tick) =>
      label(tick.label, px(tick.value), bottom + 18, {
        size: 10,
        tone: ink.muted,
        anchor: "middle",
      }),
    ),
    label(spec.x.label, right, axisY + 2, { size: 11, tone: ink.muted, anchor: "end" }),
    label(spec.y.label, left - 8, top - 10, { size: 11, tone: ink.muted }),
  ];

  const depth: Record<Tone, number> = { comparison: 0, radix: 1, primary: 2 };
  const curves = spec.curves.toSorted((a, b) => depth[a.tone] - depth[b.tone]);
  for (const curve of curves) {
    const points = curve.points.map(([x, y]) => [px(x), py(y)]);
    // Tessera is the subject: a heavy line with its measured points. Other sorts are context,
    // thin lines without points, still coloured by kind.
    const focus = curve.tone === "primary";
    marks.push(
      Plot.line(points, {
        stroke: ink[curve.tone],
        strokeWidth: focus ? 3 : 1.25,
        strokeOpacity: focus ? 1 : 0.7,
        strokeLinejoin: "round",
      }),
      Plot.dot(
        focus ? curve.points.filter(([, y]) => !below(y)).map(([x, y]) => [px(x), py(y)]) : [],
        { r: 4, fill: ink[curve.tone], stroke: ink.panel, strokeWidth: 1.5 },
      ),
      Plot.dot(
        curve.points.filter(([, y]) => below(y)).map(([x]) => [px(x), bottom]),
        { symbol: "triangle", rotate: 180, r: 4.5, fill: ink[curve.tone], stroke: ink.panel },
      ),
    );
  }
  // References draw over the measurements: a sort that copies its input lies on the input line.
  for (const reference of spec.references) {
    const points = reference.points
      .filter(([, y]) => y >= spec.y.low && y <= spec.y.high)
      .map(([x, y]) => [px(x), py(y)]);
    marks.push(
      Plot.line(points, {
        stroke: ink.axis,
        strokeWidth: 1.25,
        strokeDasharray: reference.dotted ? "1 4" : "4 4",
        strokeLinecap: "round",
      }),
    );
  }

  // Name every line at its right end, spreading crowded labels with a leader to their line.
  const placed = dodge(
    rows.map(([y]) => y),
    top + 4,
    bottom,
  );
  rows.forEach(([y, group], index) => {
    marks.push(
      Plot.line(
        [
          [right + 4, y],
          [right + 14, placed[index]],
          [right + 18, placed[index]],
        ],
        { stroke: ink.axis, strokeWidth: 0.75 },
      ),
    );
    let x = right + 22;
    group.forEach((curve, position) => {
      const text = curve.name + (position < group.length - 1 ? separator : "");
      marks.push(
        label(text, x, placed[index] + 4, {
          size: 12,
          tone: ink[curve.tone],
          strong: curve.tone === "primary",
        }),
      );
      x += textWidth(text, 12) + (curve.tone === "primary" ? 2 : 0);
    });
  });

  const legend: [CurveSwatch[], string][] = clipped
    ? [...spec.legend, [["edge"], "below the axis, value in table"]]
    : spec.legend;
  let x = pad;
  for (const [swatches, text] of legend) {
    for (const swatch of swatches) {
      marks.push(
        swatch === "edge"
          ? Plot.dot([[x + 4, legendY - 4]], {
              symbol: "triangle",
              rotate: 180,
              r: 4.5,
              fill: ink.muted,
            })
          : swatch === "dashed" || swatch === "dotted"
            ? Plot.line(
                [
                  [x - 2, legendY - 4],
                  [x + 12, legendY - 4],
                ],
                {
                  stroke: ink.axis,
                  strokeWidth: 1.25,
                  strokeDasharray: swatch === "dotted" ? "1 4" : "4 4",
                  strokeLinecap: "round",
                },
              )
            : Plot.line(
                [
                  [x - 2, legendY - 4],
                  [x + 12, legendY - 4],
                ],
                { stroke: ink[swatch], strokeWidth: 3 },
              ),
      );
      x += swatch === "edge" ? 10 : 18;
    }
    marks.push(label(text, x + 4, legendY, { size: 11, tone: ink.muted }));
    x += 4 + textWidth(text, 11) + 18;
  }
  noteLines.forEach((line, index) => {
    marks.push(label(line, pad, legendY + 22 + index * 18, { size: 11, tone: ink.muted }));
  });

  return `<!-- SPDX-License-Identifier: BSD-2-Clause -->\n${renderFigure(
    {
      width,
      height,
      id: spec.id,
      title: spec.title,
      description: `${spec.guide}. ${spec.curves
        .map((curve) => `${curve.name}: ${curve.points.map(([, y]) => y).join(", ")}`)
        .join("; ")}`,
    },
    [{ marks }],
  )}\n`;
}
