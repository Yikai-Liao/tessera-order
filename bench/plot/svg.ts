// SPDX-License-Identifier: BSD-2-Clause

import { theme } from "./theme.ts";

const { palette, font } = theme;

/** Escape text for SVG content and attribute values. */
export function escapeXml(text: string) {
  const entities: Record<string, string> = {
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
  };
  return text.replace(/[&<>"]/g, (c) => entities[c]);
}

/** Estimate Inter text width without a font engine. */
export function textWidth(text: string, px: number) {
  let em = 0;
  for (const c of text) {
    em += /[il.,:;'·| ]/.test(c)
      ? 0.28
      : /[{}()[\]frtj]/.test(c)
        ? 0.36
        : /[mwMW%×]/.test(c)
          ? 0.86
          : /[0-9]/.test(c)
            ? 0.62
            : /[A-Z]/.test(c)
              ? 0.68
              : 0.55;
  }
  return em * px;
}

export const chartStyles = `<style>
  .panel { fill: ${palette.light.panel}; stroke: ${palette.light.border}; } .ring { stroke: ${palette.light.panel}; }
  .ink { fill: ${palette.light.ink}; } .muted { fill: ${palette.light.muted}; } .rule { stroke: ${palette.light.rule}; } .axis { stroke: ${palette.grey}; }
  .tag { fill: none; stroke: ${palette.light.tag}; }
  .slower { fill: ${palette.orange}; } .faster { fill: ${palette.blue}; }
  .stem-slower { stroke: ${palette.orange}; } .stem-faster { stroke: ${palette.blue}; }
  .within { fill: ${palette.light.panel}; stroke: ${palette.grey}; }
  text { font-family: ${font.family}; font-size: ${font.body}px; font-feature-settings: "tnum", "cv11"; }
  .val { font-size: ${font.value}px; } .sub { font-size: ${font.caption}px; } .tiny { font-size: ${font.tiny}px; } .strong { font-weight: 600; }
  .hero { font-size: ${font.headline}px; font-weight: 600; letter-spacing: -0.6px; font-feature-settings: "pnum"; }
  .stat { font-size: ${font.title}px; font-weight: 600; }
  .caps { font-size: ${font.caps}px; font-weight: 600; letter-spacing: 1.1px; }
  @media (prefers-color-scheme: dark) {
    .panel { fill: ${palette.dark.panel}; stroke: ${palette.dark.border}; } .ring { stroke: ${palette.dark.panel}; }
    .ink { fill: ${palette.dark.ink}; } .muted { fill: ${palette.dark.muted}; } .rule { stroke: ${palette.dark.rule}; } .axis { stroke: ${palette.dark.axis}; }
    .tag { stroke: ${palette.dark.tag}; }
    .within { fill: ${palette.dark.panel}; stroke: ${palette.dark.axis}; }
  }
</style>`;
