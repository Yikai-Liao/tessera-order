// SPDX-License-Identifier: BSD-2-Clause

import { svgWriter } from "./layout.ts";
import { chartStyles, escapeXml } from "./svg.ts";
import { theme } from "./theme.ts";

interface FigureOptions {
  width: number;
  height: number;
  title: string;
  description: string;
  styles?: string;
  id?: string;
}

/** Create the common accessible SVG surface for a chart template. */
export function createFigure(options: FigureOptions) {
  const { width, height, title, description, styles = "", id } = options;
  const titleId = id ? `${id}-title` : "t",
    descriptionId = id ? `${id}-description` : "d";
  const { out, t } = svgWriter();
  out.push(
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}" width="${width}" height="${height}" role="img" aria-labelledby="${escapeXml(titleId)} ${escapeXml(descriptionId)}">`,
    `<title id="${escapeXml(titleId)}">${escapeXml(title)}</title>`,
    `<desc id="${escapeXml(descriptionId)}">${escapeXml(description)}</desc>`,
    chartStyles,
  );
  if (styles) out.push(styles);
  out.push(
    `<rect class="panel" x="0.5" y="0.5" width="${width - 1}" height="${height - 1}" rx="${theme.layout.panelRadius}"/>`,
  );
  return { out, t, finish: () => [...out, "</svg>"].join("\n") };
}
