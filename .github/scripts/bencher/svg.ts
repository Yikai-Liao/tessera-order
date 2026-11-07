// SPDX-License-Identifier: BSD-2-Clause

export function escapeXml(text: string) {
  const entities: Record<string, string> = {
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
  };
  return text.replace(/[&<>"]/g, (c) => entities[c]);
}

// Approximate advances of Inter in em, enough to lay out a label column
// without a font engine.
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
  .panel { fill: #fafaf8; stroke: #d8d7d2; } .ring { stroke: #fafaf8; }
  .ink { fill: #0b0b0b; } .muted { fill: #6b6a66; } .rule { stroke: #e6e5e0; } .axis { stroke: #8d8c86; }
  .tag { fill: none; stroke: #c9c8c2; }
  .slower { fill: #e8622f; } .faster { fill: #4a80e6; }
  .stem-slower { stroke: #e8622f; } .stem-faster { stroke: #4a80e6; }
  .within { fill: #fafaf8; stroke: #8d8c86; }
  text { font-family: Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif; font-size: 13px; font-feature-settings: "tnum", "cv11"; }
  .val { font-size: 12px; } .sub { font-size: 11px; } .tiny { font-size: 10px; } .strong { font-weight: 600; }
  .hero { font-size: 28px; font-weight: 600; letter-spacing: -0.6px; font-feature-settings: "pnum"; }
  .stat { font-size: 18px; font-weight: 600; }
  .caps { font-size: 10.5px; font-weight: 600; letter-spacing: 1.1px; }
  @media (prefers-color-scheme: dark) {
    .panel { fill: #1e1e1d; stroke: #3a3a37; } .ring { stroke: #1e1e1d; }
    .ink { fill: #e8e7e3; } .muted { fill: #a3a29b; } .rule { stroke: #30302e; } .axis { stroke: #6f6e69; }
    .tag { stroke: #4a4a46; }
    .within { fill: #1e1e1d; stroke: #6f6e69; }
  }
</style>`;
