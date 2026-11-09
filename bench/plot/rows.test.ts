// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import { test } from "node:test";
import { parseSvg, textPosition } from "../svg-test-utils.ts";
import { type RowChart, renderRows } from "./rows.ts";

const fixture = (): RowChart => ({
  id: "example",
  title: "A < B & C",
  guide: "One shared axis",
  columns: ["Value"],
  groups: [
    {
      title: "Examples",
      rows: [
        {
          name: { input: "row", sub: "", tag: "" },
          swarm: true,
          marks: [
            { value: 1, tone: "primary" },
            { value: 1, tone: "comparison" },
          ],
          cells: ["1.0×"],
        },
      ],
    },
  ],
  scale: { low: 0.5, high: 2, origin: 1, ticks: [{ value: 1, label: "reference" }], clamp: true },
  legend: [[["primary"], "primary series"]],
  axis: ["less", "more"],
});

test("row charts are pure and accessible", () => {
  const input = fixture(),
    original = structuredClone(input);
  const svg = renderRows(input);
  assert.equal(renderRows(input), svg);
  assert.deepEqual(input, original);
  assert.match(svg, /A &lt; B &amp; C/);
  assert.match(svg, /aria-labelledby="example-title example-description"/);
  assert.match(svg, /prefers-color-scheme: dark/);
  assert.doesNotMatch(svg, /NaN|Infinity/);
});

test("invalid geometry is rejected", () => {
  for (const value of [0, -1, NaN, Infinity]) {
    const input = fixture();
    input.groups[0].rows[0].marks[0].value = value;
    assert.throws(() => renderRows(input), /finite plot values/);
  }
  const input = fixture();
  input.groups[0].rows[0].cells = [];
  assert.throws(() => renderRows(input), /matching value cells/);
  input.scale.low = 0;
  assert.throws(() => renderRows(input), /positive, increasing scale/);
});

test("long labels expand the panel", () => {
  const input = fixture();
  input.groups[0].rows[0].name.input = "A long label ".repeat(12);
  const svg = renderRows(input);
  const width = Number(svg.match(/viewBox="0 0 (\d+)/)?.[1]);
  assert.ok(width > 720);
  assert.doesNotMatch(svg, /NaN|Infinity/);
});

test("overflow symbols survive collision layout", () => {
  for (const swarm of [false, true])
    for (const tone of ["primary", "comparison"] as const)
      for (const value of [0.25, 0.5, 1, 2, 4]) {
        const input = fixture();
        input.legend = { note: "" };
        input.groups[0].rows[0].swarm = swarm;
        input.groups[0].rows[0].marks = [{ value, tone }];
        const root = parseSvg(renderRows(input));
        const marks = [
          ...root.querySelectorAll('[aria-label="dot"] circle, [aria-label="dot"] path'),
        ];
        const visible = marks.filter((mark) => mark.getAttribute("fill") !== "none");
        assert.equal(visible.length, 1);
        const mark = visible[0];
        if (value < input.scale.low || value > input.scale.high) {
          assert.match(
            mark.getAttribute("transform") ?? "",
            value < input.scale.low ? /rotate\(270\)/ : /rotate\(90\)/,
          );
          assert.equal(mark.tagName.toLowerCase(), "path");
          assert.doesNotMatch(mark.getAttribute("d") ?? "", /A/);
        } else {
          const angle = mark.getAttribute("transform")?.match(/rotate\(([^)]+)\)/)?.[1] ?? "0";
          assert.equal(Number(angle), 0);
          assert.ok(
            mark.tagName.toLowerCase() === "circle" || (mark.getAttribute("d") ?? "").includes("A"),
          );
        }
      }
});

test("span endpoints disclose overflow", () => {
  for (const [span, rotations] of [
    [[0.5, 2], []],
    [[0.25, 2], [270]],
    [[0.5, 4], [90]],
    [
      [0.25, 4],
      [270, 90],
    ],
  ] as [number[], number[]][]) {
    const input = fixture();
    const row = input.groups[0].rows[0];
    row.swarm = false;
    row.marks = [{ value: 1, tone: "primary" }];
    row.span = [span[0], span[1]];
    input.scale.ticks = [
      { value: input.scale.low, label: "low limit" },
      { value: 1, label: "reference" },
      { value: input.scale.high, label: "high limit" },
    ];
    input.legend = { note: "" };
    const root = parseSvg(renderRows(input));
    const triangles = [...root.querySelectorAll('[aria-label="dot"] path')];
    assert.deepEqual(
      triangles.map((mark) =>
        Number(mark.getAttribute("transform")?.match(/rotate\(([^)]+)\)/)?.[1]),
      ),
      rotations,
      String(span),
    );
    const median = root.querySelectorAll('[aria-label="dot"] circle');
    assert.equal(median.length, 1);
    const ticks = [...root.querySelectorAll('[aria-label="x-axis tick label"] text')];
    for (const triangle of triangles) {
      const rotation = Number(triangle.getAttribute("transform")?.match(/rotate\(([^)]+)\)/)?.[1]);
      const boundary = ticks.find(
        (tick) => tick.textContent === (rotation === 270 ? "low limit" : "high limit"),
      );
      assert.ok(boundary);
      const position = textPosition(triangle);
      assert.ok(
        Math.abs(position.x - textPosition(boundary).x) < 1e-6,
        "endpoint is at axis limit",
      );
      assert.equal(position.y, Number(median[0].getAttribute("cy")), "endpoint stays on its row");
      assert.notEqual(
        position.x,
        Number(median[0].getAttribute("cx")),
        "endpoint is not at median",
      );
    }
  }
});

test("plotting modules have no host dependencies", async () => {
  const directory = new URL(".", import.meta.url);
  for (const file of await readdir(directory)) {
    if (!file.endsWith(".ts") || file.endsWith(".test.ts")) continue;
    const source = await readFile(new URL(file, directory), "utf8");
    assert.doesNotMatch(source, /from ["'](?:node:|.*continuous|.*report)/, file);
    assert.doesNotMatch(source, /\b(?:process|document|window)\./, file);
  }
});
