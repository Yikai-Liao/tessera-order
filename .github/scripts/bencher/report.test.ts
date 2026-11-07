// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync } from "node:fs";
import * as report from "./report.ts";
import { chartSvg } from "./chart.ts";
import { render } from "./markdown.ts";
import { fixture, hash, baseHash } from "./fixtures/report.ts";

test("classifies alerts by direction and counts missing baselines", () => {
  const s = report.summarize(fixture());
  assert.equal(s.regressions, 2);
  assert.equal(s.improvements, 1);
  assert.equal(s.missing, 1);
  assert.equal(s.hash, hash);
  assert.equal(s.baseHash, baseHash);
  // Largest slowdown first, then speedups, then cases without a baseline.
  assert.deepEqual(s.rows.map(r => r.verdict),
    ["regression", "regression", "improvement", "missing", "within"]);
  assert.equal(s.rows[0].name, "rust/row_u32/random/48/s1/api");
});

test("ignores dismissed alerts", () => {
  const r = fixture();
  r.alerts[0].status = "dismissed";
  assert.equal(report.summarize(r).regressions, 1);
});

test("names cases in words and keeps the ID for copying", () => {
  assert.equal(report.caseLabel("rust/u64/sorted99/65536/s1/api"), "u64 · 99% sorted · 65,536");
  assert.deepEqual(report.caseParts("rust/row_u64/random/4097/s1/workspace"),
    { type: "{u64,u32}", input: "random", n: "4,097", extra: "workspace", seed: "s1", mode: "workspace" });
  assert.equal(report.caseLabel("rust/u32/d16/48/s2/api"), "u32 · 16 values · 48, seed 2");
  assert.equal(report.caseLabel("not/a/benchmark"), "not/a/benchmark");
});

test("leads with counts and call-outs, then both charts and the tables", () => {
  const body = render(report.summarize(fixture()),
    { language: "rust", runUrl: "https://run", baseRef: "main", chartRef: "./chart.svg", trendRef: "./trend.svg" });
  assert.equal(body.split("\n")[0], "### Benchmarks · rust · 2 slower, 1 faster, 1 new");
  const chart = body.indexOf("](./chart.svg)");
  const trend = body.indexOf("](./trend.svg)");
  const caution = body.indexOf("> [!CAUTION]");
  const tip = body.indexOf("> [!TIP]");
  assert.ok(caution > 0 && caution < tip && tip < chart && chart < trend && trend < body.indexOf("#### Slower"));
  assert.equal(body.slice(body.indexOf("](./chart.svg)")).split("\n").slice(0, 3).join("\n"),
    "](./chart.svg)\n\n![PR benchmark history](./trend.svg)");
  assert.match(body, /Worst: \{u32,u32\} · random · 48, 726 ns → \*\*800 ns\*\*, \*\*\+10\.2%\*\*/);
  assert.match(body, /1 case faster, 1 by 20% or more\.\*\* Best: u64 · 99% sorted · 1,000,000.*\*\*−29\.1%\*\*/);
  assert.match(body, /\*\*Overall run time −[\d.]+%\*\* against `main`, the geometric mean over 4 cases/);
  assert.match(body, /<sub>aaaaaaa → `main` bbbbbbb · `intel-v1`/);
  // Names link to history, with the benchmark ID as the link title.
  assert.match(body, /\| \[random\]\(https:\/\/bencher\.dev\/perf\/proj\?[^ ]+ "rust\/u64\/random\/49\/s1\/api"\) \| u64 \| 49 \| api \| 709 ns \| 736 ns \| \+3\.8% \|/);
  // Both directions read as the change in run time; 20% or more is bold.
  assert.match(body, /\| \*\*−29\.1%\*\* \|/);
  assert.ok(!body.includes("/console/"), "links must not require a login");
  assert.match(body, /\[JSON\]\(https:\/\/api\.bencher\.dev\/v0\/projects\/proj\/reports\/r-1\)/);
  assert.match(body, /\[History\]\(https:\/\/api\.bencher\.dev\/v0\/projects\/proj\/perf\?branches=br-main%2Cbr-pr&/);
  const folded = body.slice(body.indexOf("<details>"));
  assert.match(folded, /All 5 cases/);
  assert.match(folded, /\| all equal \| u64 \| 65,536 \| api \| 26\.0 µs \| 23\.7 µs – 28\.4 µs \|.*\| `rust\/u64\/equal\/65536\/s1\/api` \|/);
  // New cases use one quiet count; details and metadata follow the tables.
  assert.match(body, /<sub>1 new case without a baseline\.<\/sub>/);
  assert.ok(body.indexOf("<sub>aaaaaaa") > body.indexOf("</details>"));
  assert.doesNotMatch(body, /Rerun|Confirm|Data, without login|The report holds|<sub>Baseline:/);
  assert.ok(!body.slice(0, body.indexOf("<details>")).includes("all equal"));
  assert.ok(!/[\u{1F300}-\u{1FAFF}\u2705\u26A0]/u.test(body), "no emoji");
});

test("omits the chart reference without an image", () => {
  const body = render(report.summarize(fixture()), { language: "rust", runUrl: "r" });
  assert.ok(!body.includes("!["));
});

test("draws the headline and one labelled mark per alerted case", () => {
  const svg = chartSvg(report.summarize(fixture()));
  assert.ok(svg);
  assert.equal(svg, readFileSync(new URL("./fixtures/chart.svg", import.meta.url), "utf8"));
  assert.match(svg, /^<svg xmlns="http:\/\/www\.w3\.org\/2000\/svg"/);
  assert.match(svg, /<text class="hero faster"[^>]*>−5\.1%<\/text>/);
  assert.match(svg, />run time overall</);
  assert.equal((svg.match(/<circle /g) ?? []).length, 3);
  // Section titles name the direction, so colour is never the only cue.
  assert.ok(svg.indexOf(">SLOWER<") < svg.indexOf(">FASTER<"));
  assert.match(svg, />−29\.1%</);
  assert.match(svg, />← less time</);
  assert.match(svg, />\{u32,u32\} · 48</);
  assert.match(svg, /prefers-color-scheme: dark/);
  const quiet = fixture();
  quiet.alerts = [];
  assert.equal(chartSvg(report.summarize(quiet)), null);
});

test("draws every alert when one direction has more than sixteen cases", () => {
  const input = fixture();
  const first = input.results[0][0];
  input.results = [Array.from({ length: 35 }, (_, index) => ({
    ...structuredClone(first),
    benchmark: { uuid: `b-${index}`, name: `rust/u64/random/${100 + index}/s1/api` },
    measures: [{ ...structuredClone(first.measures[0]), metric: { uuid: `m-${index}`, value: 900 } }],
  }))];
  input.alerts = Array.from({ length: 35 }, (_, index) => ({ metric: { uuid: `m-${index}` }, limit: "upper" }));
  const summary = report.summarize(input);
  const svg = chartSvg(summary);
  assert.ok(svg);
  assert.equal(summary.regressions, 35);
  assert.equal((svg.match(/<circle /g) ?? []).length, 35);
  assert.match(svg, />u64 · 134</);
});

test("folds small speedups into one line but draws every slowdown", () => {
  const input = fixture();
  // all equal · 65,536 is 0.1% faster than main.
  input.alerts.push({ metric: { uuid: "metric-rust/u64/equal/65536/s1/api" }, limit: "lower", status: "active" });
  const summary = report.summarize(input);
  const svg = chartSvg(summary);
  assert.ok(svg);
  assert.equal(summary.improvements, 2);
  assert.match(svg, />1 more faster, each by less than 5%</);
  assert.doesNotMatch(svg, />−0\.1%</);
  // A 3.8% slowdown stays drawn although it is under the same 5%.
  assert.match(svg, />\+3\.8%</);
  assert.equal((svg.match(/<circle /g) ?? []).length, 3);
});

test("draws small speedups when folding them would leave nothing to draw", () => {
  const input = fixture();
  input.alerts = [{ metric: { uuid: "metric-rust/u64/equal/65536/s1/api" }, limit: "lower", status: "active" }];
  const svg = chartSvg(report.summarize(input));
  assert.ok(svg);
  assert.match(svg, />−0\.1%</);
  assert.doesNotMatch(svg, /more faster/);
});

test("keeps ticks at least sixty pixels apart for a large regression", () => {
  const input = fixture();
  const measure = input.results[0][0].measures[0];
  assert.ok(measure.boundary);
  measure.metric.value = measure.boundary.baseline * 1001;
  const svg = chartSvg(report.summarize(input));
  assert.ok(svg);
  const ticks = [...svg.matchAll(/<line class="rule" x1="([\d.-]+)" x2="([\d.-]+)"/g)]
    .filter(match => match[1] === match[2]).map(match => Number(match[1])).sort((a, b) => a - b);
  assert.ok(ticks.length > 0 && ticks.length <= 10);
  assert.ok(ticks.slice(1).every((tick, index) => tick - ticks[index] >= 59.9));
});

test("reports a quiet run as such", () => {
  const r = fixture();
  r.alerts = [];
  const body = render(report.summarize(r), { language: "rust", runUrl: "r" });
  assert.equal(body.split("\n")[0], "### Benchmarks · rust · 1 new, no significant change");
  assert.ok(!body.includes("[!CAUTION]") && !body.includes("[!TIP]"));
});

test("formats latency and change", () => {
  assert.equal(report.formatTime(736), "736 ns");
  assert.equal(report.formatTime(733100), "733 µs");
  assert.equal(report.formatTime(1033988), "1.03 ms");
  assert.equal(report.formatChange(0.038), "+3.8%");
  assert.equal(report.formatChange(-0.291), "−29.1%");
  assert.equal(report.formatChange(null), "—");
});
