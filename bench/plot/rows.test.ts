// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import { test } from "node:test";
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
            { value: 1, tone: "tessera" },
            { value: 1, tone: "comparison" },
          ],
          cells: ["1.0×"],
        },
      ],
    },
  ],
  scale: { low: 0.5, high: 2, origin: 1, ticks: [{ value: 1, label: "reference" }], clamp: true },
  legend: [[["tessera"], "primary series"]],
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

test("plotting modules have no host dependencies", async () => {
  const directory = new URL(".", import.meta.url);
  for (const file of await readdir(directory)) {
    if (!file.endsWith(".ts") || file.endsWith(".test.ts")) continue;
    const source = await readFile(new URL(file, directory), "utf8");
    assert.doesNotMatch(source, /from ["'](?:node:|.*continuous|.*report)/, file);
    assert.doesNotMatch(source, /\b(?:process|document|window)\./, file);
  }
});
