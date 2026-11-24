// SPDX-License-Identifier: BSD-2-Clause

import { catalog } from "../../../../bench/catalog.ts";
import { toneOf } from "../../../../bench/report/implementation-tone.ts";

/** Catalog presentation for comparison controls, serialized by VitePress at build time. */
function load() {
  return catalog.implementations.map(({ id, label }) => ({ id, label, tone: toneOf(id) }));
}

export type ImplementationChoice = ReturnType<typeof load>[number];
export declare const data: ImplementationChoice[];
export default { watch: ["../../../../bench/catalog.toml"], load };
