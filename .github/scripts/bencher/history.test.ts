// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { loadHistory, summarizeTrend } from "./history.ts";
import { trendSvg } from "./trend-chart.ts";
import { baseHash, fixture, hash } from "./fixtures/report.ts";

// One report per run. Every case has main's baseline of 100 unless the
// test sets another.
function measured(index: number, values: number[], commit = hash) {
  const report = fixture();
  report.uuid = `report-${index}`;
  report.start_time = new Date(Date.UTC(2026, 10, 7, 0, index)).toISOString();
  report.branch.head.version = { hash: commit };
  const template = report.results[0][0];
  report.results = [values.map((value, i) => ({
    benchmark: { uuid: `case-${i}`, name: `rust/u64/random/${100 + i}/s1/api` },
    measures: [{ ...structuredClone(template.measures[0]), metric: { uuid: `metric-${index}-${i}`, value },
      boundary: { baseline: 100, lower_limit: 99, upper_limit: 101 } }],
  }))];
  report.alerts = [];
  return report;
}

function baselines(report: ReturnType<typeof measured>, values: (number | null)[]) {
  report.results[0].forEach((result, i) => {
    const value = values[i];
    result.measures[0].boundary = value === null ? null : { baseline: value, lower_limit: value, upper_limit: value };
  });
}

test("compares every run with main's latest baseline, ignoring the baselines of earlier runs", () => {
  const first = measured(1, [100, 200, 300]);
  baselines(first, [9000, 9000, 9000]);
  const current = measured(2, [50, 400]);
  const trend = summarizeTrend([first, current], current, "rust")!;
  assert.equal(trend.sharedCases, 2);
  assert.equal(trend.base, baseHash);
  // Against main's 100: run 1 is 1× and 2×, run 2 is 0.5× and 4×.
  assert.deepEqual(trend.overall.map(v => Math.round(v * 1000) / 1000), [0.414, 0.414]);
  const growing = trend.focus.find(series => series.name.endsWith("/101/s1/api"))!;
  assert.equal(growing.baseValue, 100);
  assert.equal(growing.lastValue, 400);
  assert.deepEqual(growing.changes, [1, 3]);
});

test("leaves cases without a main baseline to the table", () => {
  const first = measured(1, [100, 100]);
  const current = measured(2, [100, 200]);
  baselines(current, [100, null]);
  const trend = summarizeTrend([first, current], current, "rust")!;
  assert.equal(trend.sharedCases, 1);
  assert.deepEqual(trend.focus, []);
  baselines(current, [null, null]);
  assert.equal(summarizeTrend([first, current], current, "rust"), null);
});

test("retains same-commit reruns but deduplicates reports and excludes other environments and future runs", () => {
  const first = measured(1, [100]);
  const current = measured(2, [90]);
  const otherSpec = measured(0, [1000]);
  otherSpec.uuid = "other-spec-report";
  otherSpec.testbed.spec = { uuid: "other-spec" };
  const otherBranch = measured(0, [1000]);
  otherBranch.uuid = "other-branch-report";
  otherBranch.branch.uuid = "other-branch";
  const otherTestbed = measured(0, [1000]);
  otherTestbed.uuid = "other-testbed-report";
  otherTestbed.testbed.uuid = "other-testbed";
  const trend = summarizeTrend([current, measured(3, [1000]), otherSpec, otherBranch, otherTestbed, first, first], current, "rust")!;
  assert.deepEqual(trend.runs.map(run => run.uuid), [first.uuid, current.uuid]);
  assert.deepEqual(trend.runs.map(run => run.hash), [hash, hash]);
  assert.ok(Math.abs(trend.overall[1] + 0.1) < 1e-12);
});

test("selects every large swing, including a regression fixed later, without a row cap", () => {
  const first = measured(1, Array(8).fill(100));
  const peak = measured(2, Array(8).fill(130));
  const current = measured(3, Array(8).fill(100));
  const trend = summarizeTrend([first, peak, current], current, "rust")!;
  assert.equal(trend.focus.length, 8);
  assert.deepEqual(trend.overall.map(v => Math.round(v * 100)), [0, 30, 0]);
});

test("a quiet history only shows overall, while an unresolved alert remains visible", () => {
  const first = measured(1, [100, 100]);
  const current = measured(2, [101, 102]);
  assert.equal(summarizeTrend([first], current, "rust")!.focus.length, 0);
  current.alerts = [{ metric: { uuid: "metric-2-0" }, limit: "upper" }];
  const trend = summarizeTrend([first], current, "rust")!;
  assert.deepEqual(trend.focus.map(row => row.name), ["rust/u64/random/100/s1/api"]);
});

test("orders rows by swing alone, whichever way a case moved and whatever its alert", () => {
  const first = measured(1, [100, 100, 100]);
  const current = measured(2, [101, 90, 120]);
  current.alerts = [{ metric: { uuid: "metric-2-0" }, limit: "upper" }];
  const trend = summarizeTrend([first], current, "rust")!;
  assert.deepEqual(trend.focus.map(row => row.name.split("/")[3]), ["102", "101", "100"]);
});

test("measures a swing between PR runs, so a case far from main does not magnify noise", () => {
  // At twice main's time, 200 → 206 is a 6-point change against main but a 3% swing.
  const first = measured(1, [200]);
  const current = measured(2, [206]);
  assert.equal(summarizeTrend([first], current, "rust")!.focus.length, 0);
});

test("zero-valued cases cannot produce infinite ratios", () => {
  const first = measured(1, [0, 100]);
  const current = measured(2, [100, 80]);
  const trend = summarizeTrend([first], current, "rust")!;
  assert.equal(trend.sharedCases, 1);
  assert.ok(trend.overall.every(Number.isFinite));
  assert.equal(summarizeTrend([measured(1, [0])], measured(2, [0]), "rust"), null);
});

test("loads all report pages across reset heads and appends the just-completed report", async () => {
  const firstPage = Array.from({ length: 100 }, (_, i) => measured(i, [100]));
  const lastPage = [measured(100, [100])];
  const current = measured(101, [100]);
  const urls: URL[] = [];
  const request: typeof fetch = async input => {
    const url = new URL(String(input));
    urls.push(url);
    return new Response(JSON.stringify(url.searchParams.get("page") === "1" ? firstPage : lastPage));
  };
  const reports = await loadHistory(current, request);
  assert.equal(reports.length, 102);
  assert.equal(reports.at(-1)!.uuid, current.uuid);
  assert.equal(urls.length, 2);
  assert.equal(urls[0].searchParams.get("branch"), current.branch.uuid);
  assert.equal(urls[0].searchParams.get("testbed"), current.testbed.uuid);
  assert.equal(urls[0].searchParams.get("expand"), "true");
  assert.equal(urls[0].searchParams.get("end_time"), String(Date.parse(current.start_time!)));
});

test("refuses failed or non-advancing history rather than presenting a partial trend", async () => {
  const current = measured(101, [100]);
  await assert.rejects(loadHistory(current, async () => new Response("", { status: 503 })), /HTTP 503/);
  const page = Array.from({ length: 100 }, (_, i) => measured(i, [100]));
  await assert.rejects(loadHistory(current, async () => new Response(JSON.stringify(page))), /did not advance/);
});

test("draws main as each row's axis, colours by side of main and labels only the end runs", () => {
  // Alternating around main's 110 crosses it on every segment.
  const reports = Array.from({ length: 100 }, (_, i) => measured(i, [i % 2 ? 120 : 100], i.toString(16).padStart(40, "0")));
  const current = reports.at(-1)!;
  baselines(current, [110]);
  const trend = summarizeTrend(reports, current, "rust")!;
  const svg = trendSvg(trend);
  assert.match(svg, /class="stem-faster"/);
  assert.match(svg, /class="stem-slower"/);
  assert.match(svg, /<line class="axis main"/);
  assert.match(svg, />main bbbbbbb</);
  assert.match(svg, />100 ns</);
  assert.match(svg, />120 ns</);
  assert.doesNotMatch(svg, />#\d+</);
  assert.doesNotMatch(svg, /SLOWER|FASTER|MOVED/);
  const labels = svg.match(/<text[^>]*>[^<]*<\/text>/g)!;
  assert.equal(labels.filter(label => />[a-f0-9]{7}</.test(label)).length, 2);
  // Overall and the case: two visible end dots each, and a hover target per other run.
  assert.equal((svg.match(/<circle /g) ?? []).length, 200);
  assert.equal((svg.match(/<circle class="hit"/g) ?? []).length, 196);
  assert.doesNotMatch(svg, /NaN|Infinity/);
});
