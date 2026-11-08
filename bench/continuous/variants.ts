// SPDX-License-Identifier: BSD-2-Clause

import { catalog } from "../catalog.ts";

export const VARIANTS = catalog.configurations;
export type Variant = (typeof VARIANTS)[number];
