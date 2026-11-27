// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import type { ComparisonData } from "../../bench/tracking/result.ts";
import {
  COMPARISON_DATA_URL,
  comparisonHeadlines,
  comparisonProvenance,
  loadComparisonSnapshot,
  parseComparisonData,
  parseMarkdownTable,
  renderComparisonMarkdown,
} from "./comparisonMarkdown.ts";

const SPEC_UUID = "eaf61c35-a989-4097-bdd0-ef1dc30a7009";
const TESSERA_HASH = "d849b6801234567890abcdef1234567890abcdef";
const OTHER_HASH = "b67122201234567890abcdef1234567890abcdef";

function makeFixture(): ComparisonData {
  const overviewFigure = {
    name: "overview",
    subject: "Overall ranking",
    summary: "Every sort on the same {u32, u32} records, ranked by speed relative to slice::sort.",
    title: "Fastest of 7 stable sorts",
    chart: {
      title: "Fastest of 7 stable sorts",
    } as ComparisonData["operations"][number]["figures"][number]["chart"],
    table: "| Sort | Speed |\n| --- | --- |\n| Tessera | 2.6× |\n| radsort | 2.5× |",
  };
  return parseComparisonData({
    spec: SPEC_UUID,
    overview: overviewFigure,
    operations: [
      {
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
            workflow: "https://github.com/Yikai-Liao/tessera-order/actions/runs/1",
            memory: true,
          },
          {
            implementation: "radsort",
            label: "radsort",
            hash: OTHER_HASH,
            time: "2026-11-23T08:00:00Z",
            workflow: "https://github.com/Yikai-Liao/tessera-order/actions/runs/2",
            memory: false,
          },
        ],
        figures: [
          overviewFigure,
          {
            name: "memory",
            title: "Grows as √n",
            summary: "Peak heap allocation beyond the input.",
            chart: {
              title: "Grows as √n",
            } as ComparisonData["operations"][number]["figures"][number]["chart"],
            kind: "curves" as const,
            table: "| Sort | 1,048,576 |\n| --- | --- |\n| Tessera | 1.2 MB |",
          },
        ],
      },
      {
        id: "select",
        label: "Stable selection",
        reference: "slice::sort · full sort",
        headline: {
          operation: "Stable | selection",
          speed: 1.04,
          rank: 2,
          of: 4,
          memoryBytes: null,
          referenceMemoryBytes: null,
          noun: "selections",
        },
        runs: [
          {
            implementation: "tessera",
            label: "Tessera",
            hash: TESSERA_HASH,
            time: "2026-11-27T12:00:00Z",
            workflow: "https://github.com/Yikai-Liao/tessera-order/actions/runs/3",
            memory: true,
          },
        ],
        figures: [
          {
            name: "overview",
            subject: "Overall ranking",
            title: "Ranked 2nd of 4 stable selections",
            chart: {
              title: "Ranked 2nd of 4 stable selections",
            } as ComparisonData["operations"][number]["figures"][number]["chart"],
            table: "",
          },
        ],
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

  const empty = parseComparisonData({ spec: null, overview: null, operations: [] });
  assert.equal(renderComparisonMarkdown(empty, "headlines"), "");
  assert.equal(
    renderComparisonMarkdown(empty, "all"),
    "No cross-implementation figure is published yet.",
  );
  assert.deepEqual(comparisonHeadlines(empty), []);
  assert.equal(comparisonProvenance([]), null);
});

test("parseMarkdownTable handles separators, escaped pipes, and empty input", () => {
  assert.deepEqual(parseMarkdownTable(" "), { head: [], body: [] });
  assert.deepEqual(
    parseMarkdownTable("| Name | Notes |\n| :--- | ---: |\n| Tessera | fast \\| stable |"),
    {
      head: ["Name", "Notes"],
      body: [["Tessera", "fast | stable"]],
    },
  );
});

test("loadComparisonSnapshot resolves local overrides safely without network fallback", async (t) => {
  const dir = await mkdtemp(join(tmpdir(), "tessera-comparison-"));
  t.after(() => rm(dir, { recursive: true, force: true }));
  const publicDir = join(dir, "public");
  await mkdir(publicDir, { recursive: true });
  const fixture = makeFixture();
  const validPath = join(dir, "comparison.json");
  await writeFile(validPath, JSON.stringify(fixture));
  await writeFile(join(dir, "bad.json"), JSON.stringify({ spec: "not-a-uuid" }));
  await writeFile(join(publicDir, "preview.json"), JSON.stringify(fixture));

  let fetchCalls = 0;
  const noFetch = (async () => {
    fetchCalls += 1;
    throw new Error("unexpected network fetch");
  }) as unknown as typeof globalThis.fetch;

  assert.deepEqual(
    await loadComparisonSnapshot({ env: { DOCS_COMPARISON_FILE: validPath }, fetch: noFetch }),
    fixture,
  );
  assert.equal(
    await loadComparisonSnapshot({
      env: { DOCS_COMPARISON_FILE: join(dir, "bad.json") },
      fetch: noFetch,
    }),
    null,
  );
  assert.deepEqual(
    await loadComparisonSnapshot({
      env: { VITE_COMPARISON_SOURCE: "/preview.json" },
      publicDir,
      fetch: noFetch,
    }),
    fixture,
  );
  assert.equal(
    await loadComparisonSnapshot({
      env: { VITE_COMPARISON_SOURCE: "/../comparison.json" },
      publicDir,
      fetch: noFetch,
    }),
    null,
  );
  assert.equal(fetchCalls, 0);
});

test("loadComparisonSnapshot fetches remote URL and degrades to null on errors or timeouts", async () => {
  const fixture = makeFixture();
  const seenUrls: string[] = [];
  const okFetch = (async (input: string | URL | Request) => {
    seenUrls.push(String(input));
    return new Response(JSON.stringify(fixture), { status: 200 });
  }) as unknown as typeof globalThis.fetch;

  assert.deepEqual(await loadComparisonSnapshot({ env: {}, fetch: okFetch }), fixture);
  assert.deepEqual(seenUrls, [COMPARISON_DATA_URL]);

  const offline = (async () => {
    throw new TypeError("offline");
  }) as unknown as typeof globalThis.fetch;
  assert.equal(await loadComparisonSnapshot({ env: {}, fetch: offline }), null);

  const notFound = (async () =>
    new Response("nope", { status: 404 })) as unknown as typeof globalThis.fetch;
  assert.equal(await loadComparisonSnapshot({ env: {}, fetch: notFound }), null);

  const timeout = ((_u: string | URL | Request, init?: RequestInit) =>
    new Promise<Response>((_res, rej) => {
      init?.signal?.addEventListener("abort", () => rej(new Error("aborted")));
    })) as unknown as typeof globalThis.fetch;
  assert.equal(await loadComparisonSnapshot({ env: {}, timeoutMs: 25, fetch: timeout }), null);
});
