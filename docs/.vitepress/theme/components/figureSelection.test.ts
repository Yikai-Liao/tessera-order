// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { parseHTML } from "linkedom/worker";
import { effectScope, nextTick, shallowRef } from "vue";
import { catalog, implementation } from "../../../../bench/catalog.ts";
import { ink } from "../../../../bench/plot/annotations.ts";
import { useFigureSelection } from "./figureSelection.ts";
import implementationChoices from "./implementationChoices.data.ts";

const implementations = implementationChoices.load();

test("page-wide previews and pins update SVGs and clearing restores Tessera", async (t) => {
  const { document } = parseHTML(`<main>
    <div data-comparison-figure="rows"><svg>
      <text data-comparison-heading="Speed">SPEED</text>
      <text data-comparison-heading="Rank">RANK</text>
      <text data-comparison-row="0" data-comparison-cell="0">4×</text>
      <text data-comparison-row="0" data-comparison-cell="1">1st</text>
      <circle data-implementation="tessera"/><circle data-implementation="rdst"/>
      <circle data-implementation="glidesort"/>
    </svg></div>
    <div data-comparison-figure="memory"><svg>
      <path data-implementation="tessera"/><path data-implementation="rdst"/>
      <path data-implementation="ips4o"/>
    </svg></div>
  </main>`);
  const scope = effectScope();
  t.after(() => scope.stop());
  const { bar, pinned, preview, pin, clear } = scope.run(() =>
    useFigureSelection(
      () => document.querySelector("main"),
      () => [
        {
          anchor: "rows",
          selection: {
            tessera: { rows: [["4×", "1st"]] },
            rdst: { rows: [["2×", "2nd"]] },
            glidesort: { rows: [["1.5×", "3rd"]] },
          },
        },
      ],
      implementations,
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
  assert.equal(bar.value, null);
  preview(null);
  await nextTick();
  readTessera();

  pinned.value = "rdst";
  await nextTick();
  readRdst();
  assert.deepEqual(bar.value, { label: "rdst", ink: ink.radix, pinnedLabel: null });
  preview(document.querySelector('circle[data-implementation="glidesort"]'));
  pinned.value = "tessera";
  await nextTick();
  readTessera();
  assert.equal(bar.value, null);

  curve.addEventListener("click", (event) => pin(event as MouseEvent));
  curve.dispatchEvent(new document.defaultView!.Event("click"));
  assert.equal(pinned.value, "rdst");
  preview(document.querySelector('circle[data-implementation="glidesort"]'));
  await nextTick();
  assert.deepEqual(text(), ["SPEED", "RANK", "1.5×", "3rd"]);
  assert.deepEqual(bar.value, { label: "glidesort", ink: ink.comparison, pinnedLabel: "rdst" });
  preview(document.querySelector('circle[data-implementation="tessera"]'));
  await nextTick();
  readTessera();
  assert.deepEqual(bar.value, { label: "Tessera", ink: ink.primary, pinnedLabel: "rdst" });
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
  assert.deepEqual(bar.value, { label: "rdst", ink: ink.radix, pinnedLabel: null });
  clear();
  await nextTick();
  readTessera();
  assert.equal(bar.value, null);
  assert.equal(pinned.value, "tessera");
});

test("parallel and overview tags supply catalog choices and can pin without selection cells", async (t) => {
  const { document } = parseHTML(`<main>
    <div data-comparison-figure="parallel"><svg>
      <text data-comparison-row="0" data-comparison-cell="0">4×</text>
      <circle data-implementation="rayon-stable"/>
    </svg></div>
    <div data-comparison-figure="overview"><svg>
      <circle data-implementation="rdst"/>
    </svg></div>
  </main>`);
  const root = shallowRef<HTMLElement | null>(null);
  const scope = effectScope();
  t.after(() => scope.stop());
  const { bar, choices, pin, clear } = scope.run(() =>
    useFigureSelection(
      () => root.value,
      () => [{ anchor: "parallel" }, { anchor: "overview" }],
      implementations,
    ),
  )!;
  const legacy = parseHTML("<main><svg><circle/></svg></main>").document;
  root.value = legacy.querySelector("main");
  await nextTick();
  assert.deepEqual(choices.value, []);
  root.value = document.querySelector("main");
  await nextTick();
  const ids = ["tessera", "rdst", "rayon-stable"];
  assert.deepEqual(
    choices.value.map(({ id, label }) => ({ id, label })),
    catalog.implementations
      .filter(({ id }) => ids.includes(id))
      .map(({ id, label }) => ({ id, label })),
  );
  for (const id of ["rayon-stable", "rdst"]) {
    const mark = document.querySelector(`[data-implementation="${id}"]`)!;
    mark.addEventListener("click", (event) => pin(event as MouseEvent));
    mark.dispatchEvent(new document.defaultView!.Event("click"));
    await nextTick();
    assert.deepEqual(bar.value, {
      label: implementation(id).label,
      ink: id === "rdst" ? ink.radix : ink.comparison,
      pinnedLabel: null,
    });
    assert.ok(mark.classList.contains("selected"));
    assert.equal(document.querySelector("text")?.textContent, "4×");
    clear();
    await nextTick();
    assert.equal(bar.value, null);
    assert.ok(!mark.classList.contains("selected"));
  }
});
