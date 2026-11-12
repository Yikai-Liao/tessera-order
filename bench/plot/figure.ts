// SPDX-License-Identifier: BSD-2-Clause

import * as Plot from "@observablehq/plot";
import { parseHTML } from "linkedom/worker";
import { ink } from "./annotations.ts";
import { theme } from "./theme.ts";

interface FigureOptions {
  width: number;
  height: number;
  title: string;
  description: string;
  id?: string;
}

/** Render Plot layers into one deterministic, accessible, standalone SVG.
 *
 * Layers share a document and typography but may have independent scales,
 * as matrix columns do. The document is private to this operation; neither
 * Node globals nor a browser's live DOM are used.
 */
export function renderFigure(options: FigureOptions, layers: Plot.PlotOptions[]) {
  const { document: svgDocument } = parseHTML("<html></html>");
  const { width, height, title, description, id } = options;
  const defaults: Plot.PlotOptions = {
    document: svgDocument,
    width,
    height,
    margin: 0,
    axis: null,
    x: { type: "identity" },
    y: { type: "identity" },
    r: { type: "identity" },
    className: "benchmark-figure",
    style: { fontFamily: theme.font.family, fontSize: `${theme.font.body}px`, background: "none" },
  };
  const root = Plot.plot({
    ...defaults,
    marks: [
      Plot.rect([{ x1: 0.5, x2: width - 0.5, y1: 0.5, y2: height - 0.5 }], {
        x1: "x1",
        x2: "x2",
        y1: "y1",
        y2: "y2",
        inset: 0,
        rx: theme.layout.panelRadius,
        fill: ink.panel,
        stroke: "var(--border)",
        strokeWidth: 1,
      }),
    ],
  });
  root.setAttribute("xmlns", "http://www.w3.org/2000/svg");
  root.setAttribute("role", "img");
  const titleId = id ? `${id}-title` : "t";
  const descriptionId = id ? `${id}-description` : "d";
  // The title names the figure; the longer description stays a description.
  root.setAttribute("aria-labelledby", titleId);
  root.setAttribute("aria-describedby", descriptionId);
  for (const [name, value, elementId] of [
    ["title", title, titleId],
    ["desc", description, descriptionId],
  ]) {
    const element = svgDocument.createElementNS("http://www.w3.org/2000/svg", name);
    element.setAttribute("id", elementId);
    element.textContent = value;
    root.appendChild(element);
  }
  const style = svgDocument.createElementNS("http://www.w3.org/2000/svg", "style");
  root.appendChild(style);
  for (const layer of layers) {
    const rendered = Plot.plot({
      ...defaults,
      ...layer,
      document: svgDocument,
      className: "benchmark-figure",
    });
    for (const child of [...rendered.children]) {
      if (child.tagName.toLowerCase() !== "style") root.appendChild(child);
    }
  }
  // Plot's clip IDs are process-global. Give this private document stable IDs
  // so rerenders are identical and inline callers can namespace their figure.
  const resources = new Map<string, string>();
  for (const element of root.querySelectorAll("clipPath[id]")) {
    const previous = element.getAttribute("id")!;
    const stable = `${id ?? "benchmark"}-clip-${resources.size}`;
    resources.set(previous, stable);
    element.setAttribute("id", stable);
  }
  for (const element of root.querySelectorAll("[clip-path]")) {
    const reference = element.getAttribute("clip-path")!;
    element.setAttribute(
      "clip-path",
      reference.replace(/url\(#([^)]+)\)/g, (match: string, previous: string) =>
        resources.has(previous) ? `url(#${resources.get(previous)})` : match,
      ),
    );
  }
  // SVG image viewers may ignore CSS variables. Literal presentation colours
  // keep the standalone image readable; ordinary CSS supplies dark mode. A
  // standalone image follows the system preference; inlined in an HTML page,
  // the page's `dark` class on <html> decides, as VitePress sets it.
  const darkRules = new Set<string>();
  const pageRules = new Set<string>();
  for (const attribute of ["fill", "stroke"] as const) {
    for (const element of root.querySelectorAll(`[${attribute}]`)) {
      const value = element.getAttribute(attribute)!;
      const token = /^var\(--([a-z]+)\)$/.exec(value)?.[1];
      if (!token) continue;
      if (!Object.hasOwn(theme.palette.light, token))
        throw new Error(`Unknown figure colour: ${token}`);
      const key = token as keyof typeof theme.palette.light;
      const className = `figure-${attribute}-${key}`;
      element.setAttribute(attribute, theme.palette.light[key]);
      element.classList.add(className);
      darkRules.add(
        `.benchmark-figure .${className} { ${attribute}: ${theme.palette.dark[key]}; }`,
      );
      pageRules.add(
        `html:not(.dark) .benchmark-figure .${className} { ${attribute}: ${theme.palette.light[key]}; }`,
      );
      pageRules.add(
        `html.dark .benchmark-figure .${className} { ${attribute}: ${theme.palette.dark[key]}; }`,
      );
    }
  }
  style.textContent = `
    .benchmark-figure { font-feature-settings: "tnum", "cv11"; }
    .benchmark-figure [font-size="10.5"] { letter-spacing: 1.1px; }
    .benchmark-figure [font-size="28"] { font-feature-settings: "pnum"; letter-spacing: -.6px; }
    @media (prefers-color-scheme: dark) { ${[...darkRules].join("\n")} }
    ${[...pageRules].join("\n")}
  `;
  return root.outerHTML;
}
