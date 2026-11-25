// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { parseHTML } from "linkedom/worker";
import { label } from "./annotations.ts";
import { type CurveChart, renderCurves } from "./curves.ts";
import { renderFigure } from "./figure.ts";
import { type RowChart, renderRows } from "./rows.ts";
import { theme } from "./theme.ts";

test("standalone figures keep readable colours without CSS variable support", () => {
  const svg = renderFigure(
    { width: 240, height: 100, title: "Timing", description: "Measured values" },
    [{ marks: [label("Serial", 24, 36)] }],
  );
  const { document } = parseHTML(svg);
  const panel = document.querySelector("g[aria-label='rect']")!;
  const text = document.querySelector("g[aria-label='text']")!;
  assert.equal(panel.getAttribute("fill"), theme.palette.light.panel);
  assert.equal(panel.getAttribute("stroke"), theme.palette.light.border);
  assert.equal(text.getAttribute("fill"), theme.palette.light.ink);
  assert.doesNotMatch(svg, /(?:fill|stroke)="var\(/);
  assert.match(svg, /prefers-color-scheme: dark/);
  assert.ok(svg.includes(theme.palette.dark.ink));
  assert.ok(svg.includes(theme.palette.dark.panel));
});

function rowChart(value = 2): RowChart {
  return {
    title: "t",
    guide: "g",
    columns: ["Speed"],
    groups: [
      {
        title: "",
        rows: [
          {
            name: { input: "a", sub: "", tag: "" },
            marks: [{ value, tone: "primary" }],
            cells: ["x"],
          },
        ],
      },
    ],
    scale: { low: 1 / 4, high: 4, ticks: [{ value: 1, label: "1" }], origin: 1, clamp: true },
    legend: [[["primary"], "Tessera"]],
    axis: ["slower", "faster"],
  };
}

test("a row chart that draws an off-scale triangle explains it in its legend", () => {
  assert.match(renderRows(rowChart(64)), /off-scale, value in table/);
  assert.doesNotMatch(renderRows(rowChart()), /off-scale/);
});

test("row SVG tags implementation dots, off-scale marks and selectable cells", () => {
  const chart = rowChart();
  const row = chart.groups[0].rows[0];
  row.swarm = true;
  row.marks[0].implementation = "tessera";
  row.marks.push(
    { value: 1.5, tone: "comparison", implementation: "std-unstable" },
    { value: 8, tone: "radix", implementation: "rdst" },
  );
  chart.groups.push({
    title: "Second group",
    rows: [{ ...row, name: { ...row.name, input: "b", sub: "subtitle" }, cells: ["y"] }],
  });
  const { document } = parseHTML(renderRows(chart));
  // The swarm includes circular paths. The legend's triangle is in a separate mark group.
  const triangles = Array.from(
    document.querySelectorAll('[aria-label="dot"]:has(path[d*="A"]) path'),
  ).filter(
    (path) =>
      path
        .getAttribute("d")
        ?.match(/[MLAZ]/g)
        ?.join("") === "MLLZ",
  );
  assert.deepEqual(
    triangles.map((path) => path.getAttribute("data-implementation")),
    ["rdst", "rdst"],
  );
  for (const id of ["tessera", "std-unstable", "rdst"])
    assert.equal(
      document.querySelectorAll(`:is(circle, path)[data-implementation="${id}"]`).length,
      2,
    );
  assert.equal(document.querySelector('[data-comparison-heading="Speed"]')?.textContent, "SPEED");
  assert.deepEqual(
    Array.from(document.querySelectorAll("[data-comparison-row-name]"), (name) => [
      name.getAttribute("data-comparison-row-name"),
      name.getAttribute("data-comparison-row-group"),
      name.textContent,
    ]),
    [
      ["0", null, "a"],
      ["1", "Second group", "b"],
    ],
  );
  assert.deepEqual(
    Array.from(document.querySelectorAll('[data-comparison-cell="0"]'), (cell) => [
      cell.getAttribute("data-comparison-row"),
      cell.textContent,
    ]),
    [
      ["0", "x"],
      ["1", "y"],
    ],
  );
});

test("long reference labels are placed clear of the neighbouring speed ticks", () => {
  const chart = rowChart();
  chart.scale.ticks = [
    { value: 0.95, label: "0.95×" },
    { value: 1, label: "slice::sort · sort_by on indices" },
    { value: 1.05, label: "1.05×" },
  ];
  const { document } = parseHTML(renderRows(chart));
  const labelY = (text: string) => {
    const element = [...document.querySelectorAll("text")].find((el) => el.textContent === text);
    const y = element?.getAttribute("transform")?.match(/^translate\([^,]+,([^)]+)\)$/)?.[1];
    assert.ok(y, `Missing position for ${text}`);
    return Number(y);
  };
  const referenceY = labelY("slice::sort · sort_by on indices");
  for (const tick of ["0.95×", "1.05×"]) {
    assert.ok(referenceY - labelY(tick) >= theme.font.tiny);
  }
});

test("multiple composition marks share one legend entry, including in a chart with a legend note", () => {
  const chart = rowChart();
  chart.legend = { note: "dot: median" };
  assert.doesNotMatch(renderRows(chart), /Tessera composition/);
  const row = chart.groups[0].rows[0];
  row.swarm = true;
  row.marks.push({ value: 1.5, tone: "composition" }, { value: 8, tone: "composition" });
  const svg = renderRows(chart);
  assert.equal(svg.match(/Tessera composition/g)?.length, 1);
  const { document } = parseHTML(svg);
  const openInk = `[fill="${theme.palette.light.panel}"][stroke="${theme.palette.blue}"]`;
  assert.equal(document.querySelectorAll(`g[aria-label="dot"] path${openInk}`).length, 2);
  assert.equal(document.querySelectorAll(`g[aria-label="dot"]${openInk} circle`).length, 1);
});

test("curves ending together share a label, and only a clipped curve adds the edge legend", () => {
  const chart = (low: number): CurveChart => ({
    title: "t",
    guide: "g",
    x: { low: 1, high: 4, ticks: [{ value: 1, label: "1" }], label: "records" },
    y: { low, high: 1000, ticks: [{ value: 10, label: "10" }], label: "bytes" },
    curves: [
      {
        name: "alpha",
        implementation: "rdst",
        tone: "radix",
        points: [
          [1, 5],
          [4, 100],
        ],
      },
      {
        name: "beta",
        implementation: "std-stable",
        tone: "comparison",
        points: [
          [1, 50],
          [4, 100],
        ],
      },
      {
        name: "Tessera",
        implementation: "tessera",
        tone: "primary",
        points: [
          [1, 20],
          [4, 40],
        ],
      },
    ],
    references: [
      {
        dotted: true,
        label: "∝ √n",
        points: [
          [1, 20],
          [4, 40],
        ],
      },
    ],
    legend: [[["primary"], "Tessera"]],
  });
  const svg = renderCurves(chart(1));
  const { document } = parseHTML(svg);
  assert.deepEqual(
    Array.from(document.querySelectorAll('[aria-label="line"] path[data-implementation]'), (path) =>
      path.getAttribute("data-implementation"),
    ),
    ["std-stable", "rdst", "tessera"],
  );
  const clipped = parseHTML(renderCurves(chart(10))).document;
  assert.ok(clipped.querySelector('[aria-label="dot"] path[data-implementation="rdst"]'));
  assert.ok(svg.includes(">alpha, <"));
  assert.ok(svg.includes(">beta<"));
  // A labelled guide is named at its end instead of in the legend.
  assert.ok(svg.includes(">∝ √n<"));
  // Lines sharing an end label name a shared growth once, and differing growths each.
  const shared = chart(1);
  shared.curves[0].growth = shared.curves[1].growth = "∝ n";
  shared.curves[1].points = shared.curves[0].points;
  assert.equal(renderCurves(shared).match(/>∝ n</g)?.length, 1);
  const differing = chart(1);
  differing.curves[0].growth = "∝ n";
  differing.curves[1].growth = "constant";
  differing.curves[1].points = differing.curves[0].points;
  const { document: both } = parseHTML(renderCurves(differing));
  const labels = [...both.querySelectorAll("svg text")].map((element) => element.textContent);
  const namesAndTags = ["alpha", "∝ n", "beta", "constant"];
  assert.deepEqual(
    labels.filter((text) => namesAndTags.includes(text!)),
    namesAndTags,
  );
  assert.doesNotMatch(svg, /below the axis/);
  assert.match(renderCurves(chart(10)), /below the axis, value in table/);
  assert.doesNotMatch(svg, /Tessera composition/);
  const withComposition = chart(1);
  withComposition.curves[0].tone = "composition";
  assert.equal(renderCurves(withComposition).match(/Tessera composition/g)?.length, 1);
});
