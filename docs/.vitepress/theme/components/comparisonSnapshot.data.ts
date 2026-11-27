// SPDX-License-Identifier: BSD-2-Clause

import { readFile } from "node:fs/promises";
import { isAbsolute, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { type ComparisonData, comparisonSchema } from "../../../../bench/tracking/result.ts";
import { COMPARISON_DATA_URL } from "../../comparisonMarkdown.ts";

const publicDir = fileURLToPath(new URL("../../../public/", import.meta.url));
const snapshotKey = Symbol.for("tessera.docs.comparison");
type SnapshotBuild = { [snapshotKey]?: Promise<ComparisonData | null> };

/** Load and validate the build snapshot; an unavailable source leaves client loading available. */
export function loadComparisonSnapshot(): Promise<ComparisonData | null> {
  const file = process.env.DOCS_COMPARISON_FILE?.trim();
  const source = process.env.VITE_COMPARISON_SOURCE?.trim() || COMPARISON_DATA_URL;
  const load = async () => {
    try {
      if (file) return comparisonSchema.parse(JSON.parse(await readFile(resolve(file), "utf8")));
      if (/^https?:\/\//i.test(source)) {
        const response = await fetch(source, { signal: AbortSignal.timeout(5000) });
        return response.ok ? comparisonSchema.parse(await response.json()) : null;
      }
      const path = resolve(publicDir, source.replace(/^\/+/, ""));
      const rel = relative(publicDir, path);
      if (!rel || rel.startsWith("..") || isAbsolute(rel)) return null;
      return comparisonSchema.parse(JSON.parse(await readFile(path, "utf8")));
    } catch {
      return null;
    }
  };
  // VitePress bundles config and data loaders separately. Keep their shared promise on
  // the active site config so HTML and Markdown use one snapshot, including offline failure.
  const config = (globalThis as { VITEPRESS_CONFIG?: SnapshotBuild }).VITEPRESS_CONFIG;
  if (!config) return load();
  config[snapshotKey] ??= load();
  return config[snapshotKey];
}

export declare const data: ComparisonData | null;
export default { load: loadComparisonSnapshot };
