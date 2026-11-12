// SPDX-License-Identifier: BSD-2-Clause

import * as Plot from "@observablehq/plot";
import { renderFigure } from "./figure.ts";
import { theme } from "./theme.ts";

export interface HistoryPoint {
  time: Date;
  value: number;
  lower: number;
  upper: number;
  series: string;
  title: string;
  href: string;
}

/** One case through time. Ranges are supplied estimates, not a historical regression test. */
export function renderHistory(points: HistoryPoint[], id = "main-history") {
  if (!points.length) return "";
  const width = theme.layout.width,
    height = 330;
  return renderFigure(
    {
      width,
      height,
      id,
      title: "Main performance history",
      description:
        "One point per completed run, with Criterion confidence intervals. Lower is faster. Select a point to open its source commit.",
    },
    [
      {
        width,
        height,
        marginLeft: 72,
        marginRight: 24,
        marginTop: 24,
        marginBottom: 48,
        axis: null,
        x: { type: "utc", label: "Measurement date (UTC)", ticks: 5 },
        y: {
          label: "Nanoseconds per sort",
          zero: true,
        },
        marks: [
          Plot.gridY({ color: "var(--rule)", opacity: 1 }),
          Plot.axisX({ color: "var(--muted)" }),
          Plot.axisY({ color: "var(--muted)" }),
          Plot.lineY(points, { x: "time", y: "value", z: "series", stroke: theme.palette.blue }),
          Plot.ruleX(points, { x: "time", y1: "lower", y2: "upper", stroke: theme.palette.blue }),
          Plot.dot(points, {
            x: "time",
            y: "value",
            r: 4,
            fill: theme.palette.blue,
            title: "title",
            href: "href",
          }),
        ],
      },
    ],
  );
}
