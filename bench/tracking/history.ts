// SPDX-License-Identifier: BSD-2-Clause

import * as Plot from "@observablehq/plot";
import { renderFigure } from "../plot/figure.ts";
import { theme } from "../plot/theme.ts";

import type { TrackingRun } from "./result.ts";

/** One case through time. Ranges are supplied estimates, not a historical regression test. */
export function renderHistory(runs: TrackingRun[], caseId: string, id = "main-history") {
  const points = runs
    .filter((run) => run.values[caseId])
    .sort((a, b) => a.time.localeCompare(b.time))
    .map((run) => ({
      time: new Date(run.time),
      value: run.values[caseId][0],
      lower: run.values[caseId][1],
      upper: run.values[caseId][2],
      series: run.protocol + "/" + run.spec,
      title: `${run.time} · ${run.hash.slice(0, 7)} · ${run.rustc} · ${run.values[caseId].join(" / ")} ns (estimate / lower / upper)`,
      href: "https://github.com/Yikai-Liao/tessera-sort/commit/" + run.hash,
    }));
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
