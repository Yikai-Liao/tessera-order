// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { parseHTML } from "linkedom/worker";
import { label } from "./annotations.ts";
import { renderFigure } from "./figure.ts";
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
