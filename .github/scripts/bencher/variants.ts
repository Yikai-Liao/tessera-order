// SPDX-License-Identifier: BSD-2-Clause

export const VARIANTS = [
  { id: "default", name: "Default", testbed: "intel-v1-hybrid-v1" },
  { id: "portable-simd", name: "Portable SIMD", testbed: "intel-v1-portable-simd-hybrid-v1" },
] as const;

export type Variant = (typeof VARIANTS)[number];
