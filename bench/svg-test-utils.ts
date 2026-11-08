// SPDX-License-Identifier: BSD-2-Clause

import { DOMParser } from "linkedom/worker";

/** Inspect the serialized figure without depending on attribute order. */
export const parseSvg = (source: string) =>
  new DOMParser().parseFromString(source, "image/svg+xml").documentElement;

/** Resolve an inherited presentation attribute on a rendered mark. */
export function svgAttribute(element: Element, name: string): string | null {
  for (let node: Element | null = element; node; node = node.parentElement) {
    const value = node.getAttribute(name);
    if (value !== null) return value;
  }
  return null;
}

/** Read the position of a text annotation, including translated Plot groups. */
export function textPosition(element: Element) {
  let x = Number(element.getAttribute("x") ?? 0),
    y = Number(element.getAttribute("y") ?? 0);
  for (let node: Element | null = element; node; node = node.parentElement) {
    for (const transform of node
      .getAttribute("transform")
      ?.matchAll(/translate\(([-\d.]+)(?:[, ]+([-\d.]+))?\)/g) ?? []) {
      x += Number(transform[1]);
      y += Number(transform[2] ?? 0);
    }
  }
  return { x, y };
}

/** Keep the legacy figure's content and canvas as a rendering-library baseline. */
export function figureContent(source: string) {
  const panel = parseSvg(source);
  return {
    viewBox: panel.getAttribute("viewBox"),
    title: panel.querySelector("title")?.textContent,
    description: panel.querySelector("desc")?.textContent,
    text: [...panel.querySelectorAll("text")]
      .map((text) => text.textContent)
      .filter(Boolean)
      .sort(),
  };
}
