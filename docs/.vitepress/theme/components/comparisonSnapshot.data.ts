// SPDX-License-Identifier: BSD-2-Clause

import { readFile } from "node:fs/promises";
import { isAbsolute, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { type CurveChart, renderCurves } from "../../../../bench/plot/curves.ts";
import { type RowChart, renderRows } from "../../../../bench/plot/rows.ts";
import { type ComparisonData, comparisonSchema } from "../../../../bench/tracking/result.ts";
import { COMPARISON_DATA_URL } from "../../comparisonMarkdown.ts";

export interface ComparisonSnapshot {
  data: ComparisonData | null;
  overridden: boolean;
}

const publicDir = fileURLToPath(new URL("../../../public/", import.meta.url));
const snapshotKey = Symbol.for("tessera.docs.comparison");
type SnapshotBuild = { [snapshotKey]?: Promise<ComparisonSnapshot> };

/** Reject chart payloads that pass loose JSON validation but fail the SVG renderers during SSR. */
function validateCharts(parsed: ComparisonData): ComparisonData {
  const figures = [
    ...(parsed.overview ? [parsed.overview] : []),
    ...parsed.operations.flatMap((op) => op.figures),
  ];
  for (const figure of figures) {
    if (figure.kind === "curves") renderCurves({ ...(figure.chart as CurveChart), id: "check" });
    else renderRows({ ...(figure.chart as RowChart), id: "check" });
  }
  return parsed;
}

/** Load and validate the build snapshot; an unavailable or unrenderable source falls back to null. */
export function loadComparisonSnapshot(): Promise<ComparisonSnapshot> {
  const file = process.env.DOCS_COMPARISON_FILE?.trim();
  const envSource = process.env.VITE_COMPARISON_SOURCE?.trim();
  const overridden = Boolean(file || envSource);
  const source = envSource || COMPARISON_DATA_URL;
  const load = async (): Promise<ComparisonSnapshot> => {
    try {
      if (file) {
        const parsed = comparisonSchema.parse(JSON.parse(await readFile(resolve(file), "utf8")));
        return { data: validateCharts(parsed), overridden };
      }
      if (/^https?:\/\//i.test(source)) {
        const response = await fetch(source, { signal: AbortSignal.timeout(5000) });
        if (!response.ok) return { data: null, overridden };
        return { data: validateCharts(comparisonSchema.parse(await response.json())), overridden };
      }
      const path = resolve(publicDir, source.replace(/^\/+/, ""));
      const rel = relative(publicDir, path);
      if (!rel || rel.startsWith("..") || isAbsolute(rel)) return { data: null, overridden };
      const parsed = comparisonSchema.parse(JSON.parse(await readFile(path, "utf8")));
      return { data: validateCharts(parsed), overridden };
    } catch {
      return { data: null, overridden };
    }
  };
  // VitePress bundles config and data loaders separately. Keep their shared promise on
  // the active site config so HTML and Markdown use one snapshot, including offline failure.
  const config = (globalThis as { VITEPRESS_CONFIG?: SnapshotBuild }).VITEPRESS_CONFIG;
  if (!config) return load();
  config[snapshotKey] ??= load();
  return config[snapshotKey];
}

export declare const data: ComparisonSnapshot;
export default { load: loadComparisonSnapshot };
