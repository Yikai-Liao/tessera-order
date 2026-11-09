// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { baseHash, fixture, hash } from "./fixtures/report.ts";
import { headline, render } from "./markdown.ts";
import * as report from "./report.ts";

test("classifies alerts by direction and counts missing baselines", () => {
  const s = report.summarize(fixture());
  assert.equal(s.regressions, 2);
  assert.equal(s.improvements, 1);
  assert.equal(s.missing, 1);
  assert.equal(s.hash, hash);
  assert.equal(s.baseHash, baseHash);
  // Largest slowdown first, then speedups, then cases without a baseline.
  assert.deepEqual(
    s.rows.map((r) => r.verdict),
    ["regression", "regression", "improvement", "missing", "within"],
  );
  assert.equal(s.rows[0].name, "rust/row_u32/random/48/s1/api");
});

test("ignores dismissed alerts", () => {
  const r = fixture();
  r.alerts[0].status = "dismissed";
  assert.equal(report.summarize(r).regressions, 1);
});

test("names cases in words and keeps the ID for copying", () => {
  assert.equal(report.caseLabel("rust/u64/sorted99/65536/s1/api"), "u64 · 99% sorted · 65,536");
  assert.deepEqual(report.caseParts("rust/row_u64/random/4097/s1/unstable"), {
    type: "{u64,u32}",
    input: "random",
    n: "4,097",
    extra: "unstable",
    seed: "s1",
    mode: "unstable",
  });
  assert.equal(report.caseLabel("rust/u32/d16/48/s2/api"), "u32 · 16 values · 48, seed 2");
  assert.equal(
    report.caseLabel("rust/u64/sparse_digits/1000000/s1/api"),
    "u64 · sparse digits · 1,000,000",
  );
  assert.equal(
    report.caseLabel("rust/row_u64/hot50/65536/s1/api"),
    "{u64,u32} · 50% one key, rest random · 65,536",
  );
  assert.equal(
    report.caseLabel("rust/f64/finite_signed/65536/s1/api"),
    "f64 · finite floats · 65,536",
  );
  assert.equal(report.caseLabel("not/a/benchmark"), "not/a/benchmark");
});

test("leads with counts and call-outs, then both charts and the tables", () => {
  const body = render(report.summarize(fixture()), {
    language: "rust",
    runUrl: "https://run",
    baseRef: "main",
    chartRef: "./chart.svg",
    trendRef: "./trend.svg",
  });
  assert.equal(body.split("\n")[0], "### Benchmarks · rust · 2 slower, 1 faster, 1 new");
  const chart = body.indexOf("](./chart.svg)");
  const trend = body.indexOf("](./trend.svg)");
  const caution = body.indexOf("> [!CAUTION]");
  const tip = body.indexOf("> [!TIP]");
  assert.ok(
    caution > 0 &&
      caution < tip &&
      tip < chart &&
      chart < trend &&
      trend < body.indexOf("#### Slower"),
  );
  assert.equal(
    body.slice(body.indexOf("](./chart.svg)")).split("\n").slice(0, 3).join("\n"),
    "](./chart.svg)\n\n![PR benchmark history](./trend.svg)",
  );
  assert.match(body, /Worst: \{u32,u32\} · random · 48, 726 ns → \*\*800 ns\*\*, \*\*\+10\.2%\*\*/);
  assert.match(
    body,
    /1 case faster, 1 by 20% or more\.\*\* Best: u64 · 99% sorted · 1,000,000.*\*\*−29\.1%\*\*/,
  );
  assert.match(
    body,
    /\*\*Overall run time −[\d.]+%\*\* against `main`, the geometric mean over 4 cases/,
  );
  assert.match(body, /<sub>aaaaaaa → `main` bbbbbbb · `intel-v1-hybrid-v1`/);
  // Names link to history, with the benchmark ID as the link title.
  assert.match(
    body,
    /\| \[random\]\(https:\/\/bencher\.dev\/perf\/proj\?[^ ]+ "rust\/u64\/random\/49\/s1\/api"\) \| u64 \| 49 \| serial \| 709 ns \| 736 ns \| \+3\.8% \|/,
  );
  // Both directions read as the change in run time; 20% or more is bold.
  assert.match(body, /\| \*\*−29\.1%\*\* \|/);
  assert.ok(!body.includes("/console/"), "links must not require a login");
  assert.match(body, /\[JSON\]\(https:\/\/api\.bencher\.dev\/v0\/projects\/proj\/reports\/r-1\)/);
  assert.match(
    body,
    /\[History\]\(https:\/\/api\.bencher\.dev\/v0\/projects\/proj\/perf\?branches=br-main%2Cbr-pr&/,
  );
  const folded = body.slice(body.indexOf("<details>"));
  assert.match(folded, /All 5 cases/);
  assert.match(
    folded,
    /\| all equal \| u64 \| 65,536 \| serial \| 26\.0 µs \| 23\.7 µs – 28\.4 µs \|.*\| `rust\/u64\/equal\/65536\/s1\/api` \|/,
  );
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

test("reports a quiet run as such", () => {
  const r = fixture();
  r.alerts = [];
  const body = render(report.summarize(r), { language: "rust", runUrl: "r" });
  assert.equal(body.split("\n")[0], "### Benchmarks · rust · 1 new, no significant change");
  assert.ok(!body.includes("[!CAUTION]") && !body.includes("[!TIP]"));
});

test("does not call a run without any baseline unchanged", () => {
  const r = fixture();
  r.alerts = [];
  for (const iteration of r.results)
    for (const result of iteration) {
      for (const measure of result.measures) measure.boundary = null;
    }
  const summary = report.summarize(r);
  assert.equal(summary.compared, 0);
  assert.equal(headline(summary), `${summary.missing} new, no baseline`);
});

test("names the configuration in a check's title", () => {
  const body = render(report.summarize(fixture()), {
    language: "rust",
    configuration: "Portable SIMD",
    runUrl: "r",
  });
  assert.equal(
    body.split("\n")[0],
    "### Benchmarks · rust · Portable SIMD · 2 slower, 1 faster, 1 new",
  );
});

test("formats latency and change", () => {
  assert.equal(report.formatTime(736), "736 ns");
  assert.equal(report.formatTime(733100), "733 µs");
  assert.equal(report.formatTime(1033988), "1.03 ms");
  assert.equal(report.formatChange(0.038), "+3.8%");
  assert.equal(report.formatChange(-0.291), "−29.1%");
  assert.equal(report.formatChange(null), "—");
});
