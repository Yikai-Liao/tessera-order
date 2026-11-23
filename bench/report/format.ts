// SPDX-License-Identifier: BSD-2-Clause

// Two significant digits, except that values near 1 keep two decimals so a
// small difference does not print as "1×". Small positive values keep
// significant digits rather than rounding to zero.
export const times = (v: number) =>
  `${v < 0.01 ? v.toPrecision(2) : v >= 100 ? v.toFixed(0) : v < 1.095 ? v.toFixed(2) : Number(v.toPrecision(2))}×`;

export function formatBytes(n: number) {
  for (const [unit, size] of [
    ["GB", 1e9],
    ["MB", 1e6],
    ["kB", 1e3],
  ] as const)
    if (n >= size) return `${Number((n / size).toPrecision(n / size >= 100 ? 3 : 2))} ${unit}`;
  return `${n} B`;
}

export const ordinal = (k: number) => {
  const teen = k % 100 >= 11 && k % 100 <= 13;
  return `${k}${teen ? "th" : (["th", "st", "nd", "rd"][k % 10] ?? "th")}`;
};
