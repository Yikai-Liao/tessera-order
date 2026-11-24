// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { parseHTML } from "linkedom/worker";
import { effectScope, nextTick } from "vue";
import { useFigureSelection } from "./figureSelection.ts";

test("page-wide previews and pins update SVGs and clearing restores Tessera", async (t) => {
  const { document } = parseHTML(`<main>
    <div data-comparison-figure="rows"><svg>
      <text data-comparison-heading="Speed">SPEED</text>
      <text data-comparison-heading="Rank">RANK</text>
      <text data-comparison-row="0" data-comparison-cell="0">4×</text>
      <text data-comparison-row="0" data-comparison-cell="1">1st</text>
      <circle data-implementation="tessera"/><circle data-implementation="rdst"/>
    </svg></div>
    <div data-comparison-figure="memory"><svg>
      <path data-implementation="tessera"/><path data-implementation="rdst"/>
      <path data-implementation="ips4o"/>
    </svg></div>
  </main>`);
  const scope = effectScope();
  t.after(() => scope.stop());
  const { pinnedEntry, preview, pin, clear } = scope.run(() =>
    useFigureSelection(
      () => document.querySelector("main"),
      () => [
        {
          anchor: "rows",
          selection: {
            tessera: { label: "Tessera", tone: "primary", rows: [["4×", "1st"]] },
            rdst: { label: "rdst", tone: "radix", rows: [["2×", "2nd"]] },
          },
        },
      ],
    ),
  )!;
  const text = () => Array.from(document.querySelectorAll("text"), (node) => node.textContent);
  const readTessera = () => {
    assert.deepEqual(text(), ["SPEED", "RANK", "4×", "1st"]);
    assert.equal(document.querySelectorAll(".selected, .dimmed, [data-selecting]").length, 0);
  };
  const readRdst = () => {
    // Headings keep their text; the figure that shows rdst's cells is marked for its colour.
    assert.deepEqual(text(), ["SPEED", "RANK", "2×", "2nd"]);
    assert.deepEqual(
      Array.from(document.querySelectorAll("[data-selecting]"), (figure) =>
        figure.getAttribute("data-comparison-figure"),
      ),
      ["rows"],
    );
    assert.equal(document.querySelectorAll('[data-implementation="rdst"].selected').length, 2);
    assert.equal(document.querySelectorAll('[data-implementation="tessera"].dimmed').length, 2);
  };
  await nextTick();
  readTessera();
  const curve = document.querySelector('path[data-implementation="rdst"]')!;
  preview(curve);
  await nextTick();
  readRdst();
  // A preview alone pins nothing.
  assert.equal(pinnedEntry.value, null);
  preview(null);
  await nextTick();
  readTessera();

  curve.addEventListener("click", (event) => pin(event as MouseEvent));
  curve.dispatchEvent(new document.defaultView!.Event("click"));
  preview(document.querySelector('circle[data-implementation="tessera"]'));
  await nextTick();
  readTessera();
  preview(null);
  await nextTick();
  readRdst();
  // An implementation the rows figure lacks leaves its cells at Tessera's values.
  preview(document.querySelector('path[data-implementation="ips4o"]'));
  await nextTick();
  assert.deepEqual(text(), ["SPEED", "RANK", "4×", "1st"]);
  assert.equal(document.querySelectorAll("[data-selecting]").length, 0);
  preview(null);
  await nextTick();
  readRdst();
  assert.equal((pinnedEntry.value as { label: string } | null)?.label, "rdst");
  clear();
  await nextTick();
  readTessera();
  assert.equal(pinnedEntry.value, null);
});
