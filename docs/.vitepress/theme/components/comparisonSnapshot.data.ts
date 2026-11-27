// SPDX-License-Identifier: BSD-2-Clause

import type { ComparisonData } from "../../../../bench/tracking/result.ts";
import { loadComparisonSnapshot } from "../../comparisonMarkdown.ts";

/**
 * Validated `comparison.json` snapshot serialized by VitePress at build time.
 *
 * Delegates source resolution, fetching, and schema validation to `comparisonMarkdown.ts`.
 * Returns `null` when offline or unavailable so builds degrade gracefully to client loading.
 */
function load(): Promise<ComparisonData | null> {
  return loadComparisonSnapshot();
}

export declare const data: ComparisonData | null;
export default { load };
