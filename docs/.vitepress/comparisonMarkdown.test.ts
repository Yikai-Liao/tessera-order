// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { loadConfigFromFile } from "vite";
import { comparisonSchema } from "../../bench/tracking/result.ts";
import {
  COMPARISON_DATA_URL,
  comparisonHeadlines,
  comparisonProvenance,
  comparisonTable,
  renderComparisonMarkdown,
} from "./comparisonMarkdown.ts";

import { loadComparisonSnapshot } from "./theme/components/comparisonSnapshot.data.ts";

const SPEC_UUID = "eaf61c35-a989-4097-bdd0-ef1dc30a7009";
const TESSERA_HASH = "d849b6801234567890abcdef1234567890abcdef";
const OTHER_HASH = "b67122201234567890abcdef1234567890abcdef";

function makeFixture() {
  const figure = {
    name: "overview",
    title: "Fastest of 7 stable sorts",
    summary: "Every sort on the same records.",
    chart: { title: "Fastest of 7 stable sorts" },
    table: "| Sort | Speed |\n| --- | --- |\n| Tessera | 2.6× |",
  };
  const sort = {
    id: "sort",
    label: "Full sort",
    reference: "slice::sort",
    headline: {
      operation: "Full sort",
      speed: 2.56,
      rank: 1,
      of: 7,
      memoryBytes: 1_236_216,
      referenceMemoryBytes: 8_000_000,
      noun: "sorts",
    },
    runs: [
      {
        implementation: "tessera",
        label: "Tessera",
        hash: TESSERA_HASH,
        time: "2026-11-27T12:00:00Z",
        memory: true,
      },
      {
        implementation: "radsort",
        label: "radsort",
        hash: OTHER_HASH,
        time: "2026-11-23T08:00:00Z",
        memory: false,
      },
    ].map((run) => ({ ...run, workflow: "https://example.test/run" })),
    figures: [
      figure,
      {
        ...figure,
        name: "memory",
        title: "Grows as √n",
        summary: "Peak heap allocation beyond the input.",
      },
    ],
  };
  return comparisonSchema.parse({
    spec: SPEC_UUID,
    overview: figure,
    operations: [
      sort,
      {
        ...sort,
        id: "select",
        label: "Stable selection",
        runs: sort.runs.slice(0, 1),
        headline: {
          ...sort.headline,
          operation: "Stable | selection",
          speed: 1.04,
          rank: 2,
          of: 4,
          memoryBytes: null,
          referenceMemoryBytes: null,
          noun: "selections",
        },
        figures: [figure],
      },
    ],
  });
}

test("renderComparisonMarkdown renders headlines, overview, all, and empty views accurately", () => {
  const data = makeFixture();

  const headlines = renderComparisonMarkdown(data, "headlines");
  assert.doesNotMatch(headlines, /^#{1,6}\s/m);
  assert.match(
    headlines,
    /\| Full sort \| Fastest of 7 stable sorts \| 2\.6× \| 1\.2 MB \(vs 8 MB\) \|/,
  );
  assert.match(
    headlines,
    /\| Stable \\\| selection \| Ranked 2nd of 4 stable selections \| 1\.04× \| — \|/,
  );

  const overview = renderComparisonMarkdown(data, "overview");
  assert.doesNotMatch(overview, /^#{1,6}\s/m);
  assert.match(overview, /^\*\*Fastest of 7 stable sorts\.\*\*/m);
  assert.match(overview, /2026-11-23 to 2026-11-27; Tessera at \[d849b68\]/);
  assert.match(overview, /Memory not yet recorded for radsort\./);

  const all = renderComparisonMarkdown(data, "all");
  assert.match(all, /^## At a glance$/m);
  assert.match(all, /^## Full sort$/m);
  assert.match(all, /^### Grows as √n\n\nPeak heap allocation beyond the input\./m);
  assert.match(all, /bare metal, 2026-11-27; Tessera at \[d849b68\]/);

  const empty = comparisonSchema.parse({ spec: null, overview: null, operations: [] });
  assert.equal(renderComparisonMarkdown(empty, "headlines"), "");
  assert.equal(
    renderComparisonMarkdown(empty, "all"),
    "No cross-implementation figure is published yet.",
  );
  assert.deepEqual(comparisonHeadlines(empty), []);
  assert.equal(comparisonProvenance([]), null);
});

test("comparisonTable handles separators, escaped pipes, and empty input", () => {
  assert.deepEqual(comparisonTable(" "), { head: [], body: [] });
  assert.deepEqual(
    comparisonTable("| Name | Notes |\n| :--- | ---: |\n| Tessera | fast \\| stable |"),
    {
      head: ["Name", "Notes"],
      body: [["Tessera", "fast | stable"]],
    },
  );
});

test("snapshot loading validates local and remote sources and preserves offline builds", async (t) => {
  const dir = await mkdtemp(join(tmpdir(), "tessera-comparison-"));
  const fixture = makeFixture();
  const file = join(dir, "comparison.json");
  await writeFile(file, JSON.stringify(fixture));
  const original = {
    DOCS_COMPARISON_FILE: process.env.DOCS_COMPARISON_FILE,
    VITE_COMPARISON_SOURCE: process.env.VITE_COMPARISON_SOURCE,
  };
  const originalFetch = globalThis.fetch;
  let response: Response | Error = new Response(JSON.stringify(fixture));
  const calls: string[] = [];
  globalThis.fetch = async (input, init) => {
    calls.push(String(input));
    assert.ok(init?.signal instanceof AbortSignal);
    if (response instanceof Error) throw response;
    return response;
  };
  t.after(async () => {
    globalThis.fetch = originalFetch;
    for (const [key, value] of Object.entries(original)) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
    await rm(dir, { recursive: true, force: true });
  });
  process.env.DOCS_COMPARISON_FILE = file;
  assert.deepEqual(await loadComparisonSnapshot(), fixture);
  await writeFile(file, "{}");
  assert.equal(await loadComparisonSnapshot(), null);
  assert.equal(calls.length, 0);
  delete process.env.DOCS_COMPARISON_FILE;
  process.env.VITE_COMPARISON_SOURCE = "/../comparison.json";
  assert.equal(await loadComparisonSnapshot(), null);
  assert.equal(calls.length, 0);
  process.env.VITE_COMPARISON_SOURCE = "https://example.test/comparison.json";
  response = new Response(JSON.stringify(fixture));
  assert.deepEqual(await loadComparisonSnapshot(), fixture);
  for (const failure of [new Response("{}"), new Response("missing", { status: 404 })]) {
    response = failure;
    assert.equal(await loadComparisonSnapshot(), null);
  }
  response = new TypeError("offline");
  assert.equal(await loadComparisonSnapshot(), null);
  delete process.env.VITE_COMPARISON_SOURCE;
  response = new Response(JSON.stringify(fixture));
  assert.deepEqual(await loadComparisonSnapshot(), fixture);
  assert.equal(calls.at(-1), COMPARISON_DATA_URL);
  const originalConfig = Reflect.get(globalThis, "VITEPRESS_CONFIG");
  t.after(() => {
    if (originalConfig === undefined) Reflect.deleteProperty(globalThis, "VITEPRESS_CONFIG");
    else Reflect.set(globalThis, "VITEPRESS_CONFIG", originalConfig);
  });
  Reflect.set(globalThis, "VITEPRESS_CONFIG", {});
  response = new Response(JSON.stringify(fixture));
  const before = calls.length;
  assert.deepEqual(await loadComparisonSnapshot(), fixture);
  const loader = await loadConfigFromFile(
    { command: "build", mode: "production" },
    fileURLToPath(new URL("./theme/components/comparisonSnapshot.data.ts", import.meta.url)),
  );
  const rebundled = loader!.config as unknown as { load: typeof loadComparisonSnapshot };
  assert.deepEqual(await rebundled.load(), fixture);
  assert.equal(
    calls.length,
    before + 1,
    "config and VitePress's separately bundled loader share one snapshot",
  );
});
