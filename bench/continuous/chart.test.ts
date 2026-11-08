// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { ink, textWidth } from "../plot/annotations.ts";
import { figureContent, parseSvg, svgAttribute, textPosition } from "../svg-test-utils.ts";
import { chartSvg } from "./chart.ts";
import { fixture } from "./fixtures/report.ts";
import { attribution, changedRows } from "./matrix.ts";
import { type BencherReport, summarize } from "./report.ts";

const one = (report: BencherReport = fixture()) => [
  { name: "Default", summary: summarize(report) },
];

// A second build of the fixture in which only the {u32,u32} slowdown is
// alerted; the other cases are within their limits.
function two() {
  const simd = fixture();
  simd.alerts = simd.alerts.filter(
    (alert) => alert.metric.uuid === "metric-rust/row_u32/random/48/s1/api",
  );
  return [...one(), { name: "Portable SIMD", summary: summarize(simd) }];
}

const marks = (svg: string, status: "slower" | "faster" | "within") =>
  [...parseSvg(svg).querySelectorAll("circle")].filter(
    (dot) =>
      svgAttribute(dot, "fill") ===
        (status === "slower" ? ink.radix : status === "faster" ? ink.primary : ink.panel) &&
      svgAttribute(dot, "stroke") === (status === "within" ? ink.axis : ink.panel),
  ).length;

test("draws one build as a column with its headline and one mark per alerted case", () => {
  const svg = chartSvg(one());
  assert.ok(svg);
  assert.deepEqual(
    figureContent(svg),
    figureContent(readFileSync(new URL("./fixtures/chart.svg", import.meta.url), "utf8")),
  );
  const panel = parseSvg(svg);
  assert.equal(panel.getAttribute("xmlns"), "http://www.w3.org/2000/svg");
  const headline = [...panel.querySelectorAll("text")].find(
    (text) => text.textContent === "−5.1%",
  )!;
  assert.equal(svgAttribute(headline, "fill"), ink.primary);
  assert.equal(svgAttribute(headline, "font-size"), "28");
  assert.match(svg, />Default</);
  assert.match(svg, />2 slower · 1 faster</);
  assert.equal(marks(svg, "slower"), 2);
  assert.equal(marks(svg, "faster"), 1);
  assert.match(svg, />−29\.1%</);
  assert.match(svg, />← less time</);
  assert.match(svg, />\{u32,u32\} · 48</);
  assert.match(svg, />STABLE</);
  assert.match(svg, /prefers-color-scheme: dark/);
  const quiet = fixture();
  quiet.alerts = [];
  assert.equal(chartSvg(one(quiet)), null);
});

test("names each case once and shows its quiet values in the other builds", () => {
  const svg = chartSvg(two())!;
  assert.match(svg, />Portable SIMD</);
  assert.equal((svg.match(/>\{u32,u32\} · 48</g) ?? []).length, 1);
  // Three cases are alerted in Default; Portable SIMD alerts one and shows
  // the other two as hollow dots within limits; the legend has one more.
  assert.equal(marks(svg, "slower"), 3);
  assert.equal(marks(svg, "within"), 2 + 1);
  const panel = parseSvg(svg);
  assert.ok(
    [...panel.querySelectorAll("text")].some(
      (text) => text.textContent === "−29.1%" && svgAttribute(text, "fill") === ink.muted,
    ),
  );
  // Both columns share one scale: equal changes are equally far from zero.
  const axes = [...panel.querySelectorAll("line")]
    .filter(
      (line) =>
        svgAttribute(line, "stroke") === ink.axis &&
        line.getAttribute("x1") === line.getAttribute("x2"),
    )
    .map((line) => Number(line.getAttribute("x1")));
  const dots = [...panel.querySelectorAll("circle")]
    .filter(
      (dot) => svgAttribute(dot, "fill") === ink.radix && svgAttribute(dot, "stroke") === ink.panel,
    )
    .map((dot) => [Number(dot.getAttribute("cx")), Number(dot.getAttribute("cy"))]);
  const row = dots.filter(([, cy]) => cy === dots[0][1]);
  assert.equal(row.length, 2);
  assert.ok(Math.abs(row[0][0] - axes[0] - (row[1][0] - axes[1])) < 0.2);
});

test("says whether an alert is shared or confined to some builds", () => {
  const configurations = two();
  const [slowest] = changedRows(configurations);
  assert.equal(slowest.name, "rust/row_u32/random/48/s1/api");
  assert.equal(attribution(configurations, [true, true]), "both");
  assert.equal(attribution(configurations, [true, false]), "Default only");
  assert.equal(attribution(configurations.slice(0, 1), [true]), null);
});

test("groups cases by API mode under one title each", () => {
  const input = fixture();
  const first = input.results[0][0];
  input.results[0].push({
    ...structuredClone(first),
    benchmark: { uuid: "b-unstable", name: "rust/u64/random/49/s1/unstable" },
    measures: [
      { ...structuredClone(first.measures[0]), metric: { uuid: "m-unstable", value: 900 } },
    ],
  });
  input.alerts.push({ metric: { uuid: "m-unstable" }, limit: "upper" });
  const svg = chartSvg(one(input))!;
  assert.ok(svg.indexOf(">STABLE<") < svg.indexOf(">UNSTABLE<"));
  assert.equal((svg.match(/>STABLE</g) ?? []).length, 1);
});

test("draws every alert when one direction has more than sixteen cases", () => {
  const input = fixture();
  const first = input.results[0][0];
  input.results = [
    Array.from({ length: 35 }, (_, index) => ({
      ...structuredClone(first),
      benchmark: { uuid: `b-${index}`, name: `rust/u64/random/${100 + index}/s1/api` },
      measures: [
        { ...structuredClone(first.measures[0]), metric: { uuid: `m-${index}`, value: 900 } },
      ],
    })),
  ];
  input.alerts = Array.from({ length: 35 }, (_, index) => ({
    metric: { uuid: `m-${index}` },
    limit: "upper",
  }));
  const svg = chartSvg(one(input))!;
  assert.equal(marks(svg, "slower"), 35);
  assert.match(svg, />u64 · 134</);
});

test("folds small speedups into one line but draws every slowdown", () => {
  const input = fixture();
  // all equal · 65,536 is 0.1% faster than main.
  input.alerts.push({
    metric: { uuid: "metric-rust/u64/equal/65536/s1/api" },
    limit: "lower",
    status: "active",
  });
  const svg = chartSvg(one(input))!;
  assert.match(svg, />1 more faster, each by less than 5%</);
  assert.doesNotMatch(svg, />−0\.1%</);
  // A 3.8% slowdown stays drawn although it is under the same 5%.
  assert.match(svg, />\+3\.8%</);
  assert.equal(marks(svg, "slower") + marks(svg, "faster"), 3);
});

test("draws small speedups when folding them would leave nothing to draw", () => {
  const input = fixture();
  input.alerts = [
    { metric: { uuid: "metric-rust/u64/equal/65536/s1/api" }, limit: "lower", status: "active" },
  ];
  const svg = chartSvg(one(input))!;
  assert.match(svg, />−0\.1%</);
  assert.doesNotMatch(svg, /more faster/);
});

test("keeps ticks legible for a large regression", () => {
  const input = fixture();
  const measure = input.results[0][0].measures[0];
  assert.ok(measure.boundary);
  measure.metric.value = measure.boundary.baseline * 1001;
  const svg = chartSvg(one(input))!;
  const ticks = [...parseSvg(svg).querySelectorAll("line")]
    .filter(
      (line) =>
        svgAttribute(line, "stroke") === ink.rule &&
        line.getAttribute("x1") === line.getAttribute("x2"),
    )
    .map((line) => Number(line.getAttribute("x1")))
    .sort((a, b) => a - b);
  assert.ok(ticks.length > 0 && ticks.length <= 12);
  assert.ok(ticks.slice(1).every((tick, index) => tick - ticks[index] >= 43.9));
  assert.doesNotMatch(svg, /NaN|Infinity/);
});

test("keeps header and legend text inside the panel", () => {
  for (const configurations of [one(), two()]) {
    const svg = chartSvg(configurations)!;
    const width = Number(svg.match(/viewBox="0 0 (\d+)/)![1]);
    const texts = [...parseSvg(svg).querySelectorAll("text")];
    const x = textPosition(texts.find((text) => text.textContent === "more time →")!).x;
    assert.ok(x <= width - 24);
    const title = textPosition(texts.find((text) => svgAttribute(text, "font-size") === "28")!).x;
    assert.ok(
      24 + textWidth("Run time against main", 13) * 1.05 < title,
      `title overlaps the first column at ${title}`,
    );
  }
});
