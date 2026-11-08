// SPDX-License-Identifier: BSD-2-Clause

/** Shared visual tokens for offline reports and browser-rendered benchmark figures. */
export const theme = {
  palette: {
    blue: "#4a80e6",
    orange: "#e8622f",
    grey: "#8d8c86",
    light: {
      panel: "#fafaf8",
      border: "#d8d7d2",
      ink: "#0b0b0b",
      muted: "#6b6a66",
      rule: "#e6e5e0",
      axis: "#8d8c86",
      tag: "#c9c8c2",
    },
    dark: {
      panel: "#1e1e1d",
      border: "#3a3a37",
      ink: "#e8e7e3",
      muted: "#a3a29b",
      rule: "#30302e",
      axis: "#6f6e69",
      tag: "#4a4a46",
    },
  },
  font: {
    family: 'Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif',
    body: 13,
    value: 12,
    caption: 11,
    tiny: 10,
    headline: 28,
    title: 18,
    caps: 10.5,
  },
  layout: { width: 720, pad: 24, row: 38, singleRow: 34, headerRule: 76, panelRadius: 12 },
} as const;
