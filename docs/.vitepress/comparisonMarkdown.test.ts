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
    table: [
      "| Sort | Kind | Stability | Speed vs slice::sort | Largest extra memory |",
      "| --- | --- | --- | --- | --- |",
      "| Tessera | radix sort | stable | 2.6× | 1.2 MB |",
      "| radsort | radix sort | stable | 2.5× | 8 MB |",
    ].join("\n"),
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
            table: [
              "| Sort | 1,024 | 1,048,576 |",
              "| --- | --- | --- |",
              "| Tessera | 20 kB | 1.2 MB |",
            ].join("\n"),
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

test("renderComparisonMarkdown overview omits section headings and includes table and provenance", () => {
  const data = makeFixture();
  const markdown = renderComparisonMarkdown(data, "overview");

  assert.doesNotMatch(markdown, /^#{1,6}\s/m);
  assert.match(markdown, /^\*\*Fastest of 7 stable sorts\.\*\* Every sort on the same/m);
  assert.match(markdown, /\| Tessera \| radix sort \| stable \| 2\.6× \| 1\.2 MB \|/);
  assert.match(
    markdown,
    /2026-11-23 to 2026-11-27; Tessera at \[d849b68\]\(https:\/\/github\.com\/Yikai-Liao\/tessera-order\/commit\/d849b6801234567890abcdef1234567890abcdef\)\./,
  );
  assert.match(markdown, /Memory not yet recorded for radsort\./);
});

test("renderComparisonMarkdown all renders headlines, sections, subjects, titles, tables, and provenance", () => {
  const data = makeFixture();
  const markdown = renderComparisonMarkdown(data, "all");

  assert.match(markdown, /^## At a glance$/m);
  assert.match(
    markdown,
    /\| Full sort \| Fastest of 7 stable sorts \| 2\.6× \| 1\.2 MB \(vs 8 MB\) \|/,
  );
  assert.match(
    markdown,
    /\| Stable \\\| selection \| Ranked 2nd of 4 stable selections \| 1\.04× \| — \|/,
  );
  assert.match(markdown, /^## Full sort$/m);
  assert.match(markdown, /^### Overall ranking$/m);
  assert.match(markdown, /^### Grows as √n$/m);
  // When `subject` is absent and `title` becomes the heading, the title is not repeated below it.
  assert.doesNotMatch(markdown, /### Grows as √n\n\n\*\*Grows as √n/);
  assert.match(markdown, /### Grows as √n\n\nPeak heap allocation beyond the input\./);
  // Single-day runs do not print a redundant "date to date" span.
  assert.match(markdown, /bare metal, 2026-11-27; Tessera at \[d849b68\]/);
});

test("renderComparisonMarkdown and helpers handle empty sections and runs gracefully", () => {
  const empty = parseComparisonData({ spec: null, overview: null, operations: [] });
  assert.equal(
    renderComparisonMarkdown(empty, "overview"),
    "No cross-implementation figure is published yet.",
  );
  assert.equal(
    renderComparisonMarkdown(empty, "all"),
    "No cross-implementation figure is published yet.",
  );
  assert.deepEqual(comparisonHeadlines(empty), []);
  assert.equal(comparisonProvenance([]), null);
});

test("parseMarkdownTable parses aligned separators, escaped pipes, and empty input", () => {
  assert.deepEqual(parseMarkdownTable("   "), { head: [], body: [] });
  const parsed = parseMarkdownTable(
    [
      "| Name | Notes |",
      "| :--- | ---: |",
      "| Tessera | fast \\| stable |",
      "| radsort | 8 MB |",
    ].join("\n"),
  );
  assert.deepEqual(parsed, {
    head: ["Name", "Notes"],
    body: [
      ["Tessera", "fast | stable"],
      ["radsort", "8 MB"],
    ],
  });
});

test("loadComparisonSnapshot reads DOCS_COMPARISON_FILE and never falls back to network on failure", async (t) => {
  const dir = await mkdtemp(join(tmpdir(), "tessera-comparison-"));
  t.after(() => rm(dir, { recursive: true, force: true }));
  const fixture = makeFixture();
  const validPath = join(dir, "comparison.json");
  const badPath = join(dir, "bad.json");
  await writeFile(validPath, JSON.stringify(fixture));
  await writeFile(badPath, JSON.stringify({ spec: "not-a-uuid" }));

  let fetchCalls = 0;
  const stubFetch = (async () => {
    fetchCalls += 1;
    throw new Error("should not fetch");
  }) as unknown as typeof globalThis.fetch;

  const loaded = await loadComparisonSnapshot({
    env: { DOCS_COMPARISON_FILE: validPath },
    fetch: stubFetch,
  });
  assert.deepEqual(loaded, fixture);

  assert.equal(
    await loadComparisonSnapshot({ env: { DOCS_COMPARISON_FILE: badPath }, fetch: stubFetch }),
    null,
  );
  assert.equal(
    await loadComparisonSnapshot({
      env: { DOCS_COMPARISON_FILE: join(dir, "missing.json") },
      fetch: stubFetch,
    }),
    null,
  );
  assert.equal(fetchCalls, 0);
});

test("loadComparisonSnapshot resolves VITE_COMPARISON_SOURCE inside publicDir and blocks traversal", async (t) => {
  const dir = await mkdtemp(join(tmpdir(), "tessera-comparison-public-"));
  t.after(() => rm(dir, { recursive: true, force: true }));
  const publicDir = join(dir, "public");
  await mkdir(publicDir, { recursive: true });
  const fixture = makeFixture();
  await writeFile(join(publicDir, "preview.json"), JSON.stringify(fixture));
  await writeFile(join(dir, "outside.json"), JSON.stringify(fixture));

  let fetchCalls = 0;
  const stubFetch = (async () => {
    fetchCalls += 1;
    throw new Error("should not fetch");
  }) as unknown as typeof globalThis.fetch;

  const loaded = await loadComparisonSnapshot({
    env: { VITE_COMPARISON_SOURCE: "/preview.json" },
    publicDir,
    fetch: stubFetch,
  });
  assert.deepEqual(loaded, fixture);

  const escaped = await loadComparisonSnapshot({
    env: { VITE_COMPARISON_SOURCE: "/../outside.json" },
    publicDir,
    fetch: stubFetch,
  });
  assert.equal(escaped, null);
  assert.equal(fetchCalls, 0);
});

test("loadComparisonSnapshot fetches remote URL and degrades to null on offline, HTTP, schema, or timeout errors", async () => {
  const fixture = makeFixture();
  const seenUrls: string[] = [];

  const okFetch = (async (input: string | URL | Request) => {
    seenUrls.push(String(input));
    return new Response(JSON.stringify(fixture), { status: 200 });
  }) as unknown as typeof globalThis.fetch;

  const loaded = await loadComparisonSnapshot({ env: {}, fetch: okFetch });
  assert.deepEqual(loaded, fixture);
  assert.deepEqual(seenUrls, [COMPARISON_DATA_URL]);

  const customUrl = "https://example.test/custom-comparison.json";
  const fromCustom = await loadComparisonSnapshot({
    env: { VITE_COMPARISON_SOURCE: customUrl },
    fetch: okFetch,
  });
  assert.deepEqual(fromCustom, fixture);
  assert.equal(seenUrls.at(-1), customUrl);

  const offlineFetch = (async () => {
    throw new TypeError("network unreachable");
  }) as unknown as typeof globalThis.fetch;
  assert.equal(await loadComparisonSnapshot({ env: {}, fetch: offlineFetch }), null);

  const httpErrorFetch = (async () =>
    new Response("not found", { status: 404 })) as unknown as typeof globalThis.fetch;
  assert.equal(await loadComparisonSnapshot({ env: {}, fetch: httpErrorFetch }), null);

  const invalidPayloadFetch = (async () =>
    new Response(JSON.stringify({ spec: null }), {
      status: 200,
    })) as unknown as typeof globalThis.fetch;
  assert.equal(await loadComparisonSnapshot({ env: {}, fetch: invalidPayloadFetch }), null);

  const timeoutFetch = ((_input: string | URL | Request, init?: RequestInit) =>
    new Promise<Response>((_resolve, reject) => {
      init?.signal?.addEventListener("abort", () => reject(new Error("aborted")));
    })) as unknown as typeof globalThis.fetch;
  assert.equal(await loadComparisonSnapshot({ env: {}, timeoutMs: 25, fetch: timeoutFetch }), null);
});
