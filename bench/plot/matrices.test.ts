// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { parseSvg } from "../svg-test-utils.ts";
import { type ChangeChart, renderChanges } from "./changes.ts";
import { renderTrends, type TrendChart } from "./trends.ts";

const common = () => ({
  id: "matrix",
  title: { input: "A < B", sub: "shared reference", tag: "" },
  headline: "A < B",
  description: "Measured changes",
  columns: [{ name: "Build", value: "+10%", detail: "one series", tone: "radix" as const }],
});
const changes = (): ChangeChart => ({
  ...common(),
  majorLabel: "5% or more",
  groups: [
    {
      title: "Cases",
      rows: [
        {
          name: common().title,
          cells: [{ change: 0.1, value: "+10%", status: "slower", major: true }],
        },
      ],
    },
  ],
});
const trends = (): TrendChart => ({
  ...common(),
  minimumSpan: 0.05,
  reference: "main",
  guide: "latest run",
  rows: [
    {
      name: common().title,
      aggregate: true,
      cells: [{ changes: [-0.1, 0, 0.1], value: "+10%", major: true }],
    },
  ],
});

test("matrix renders preserve inputs", () => {
  const check = <T>(spec: T, render: (spec: T) => string) => {
    const original = structuredClone(spec);
    const svg = render(spec);
    assert.equal(render(spec), svg);
    assert.deepEqual(spec, original);
    assert.match(svg, /A &lt; B/);
  };
  check(changes(), renderChanges);
  check(trends(), renderTrends);
});

test("trend clips resolve locally", () => {
  const first = parseSvg(renderTrends(trends()));
  const secondSpec = trends();
  secondSpec.id = "another";
  const second = parseSvg(renderTrends(secondSpec));
  const ids = new Set([...first.querySelectorAll("[id]")].map((element) => element.id));
  for (const element of second.querySelectorAll("[id]")) assert.ok(!ids.has(element.id));
  for (const root of [first, second]) {
    for (const element of root.querySelectorAll("[clip-path]")) {
      const reference = element.getAttribute("clip-path")!.slice(5, -1);
      assert.ok([...root.querySelectorAll("clipPath")].some((clip) => clip.id === reference));
    }
  }
});

test("short histories stay finite", () => {
  for (const values of [[0], [0, 0], [-0.1], [0.1]]) {
    const spec = trends();
    spec.rows[0].cells[0]!.changes = values;
    assert.doesNotMatch(renderTrends(spec), /NaN|Infinity/);
  }
  const spec = trends();
  spec.rows[0].cells[0] = null;
  assert.match(renderTrends(spec), />—</);
});

test("matrix geometry rejects gaps", () => {
  const change = changes();
  change.groups[0].rows[0].cells = [];
  assert.throws(() => renderChanges(change), /matching cells/);
  const trend = trends();
  trend.rows[0].cells[0]!.changes = [];
  assert.throws(() => renderTrends(trend), /finite histories/);
  trend.rows[0].cells[0]!.changes = [Infinity];
  assert.throws(() => renderTrends(trend), /finite histories/);
});
