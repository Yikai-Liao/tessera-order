// SPDX-License-Identifier: BSD-2-Clause

const assert = require("node:assert/strict");
const test = require("node:test");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const report = require("./bencher_report.cjs");

const hash = "a".repeat(40);
const baseHash = "b".repeat(40);
const latency = { uuid: "m-latency", name: "Latency" };
const model = {
  test: "t_test", min_sample_size: 5, max_sample_size: 32,
  lower_boundary: 0.999, upper_boundary: 0.999,
};

function result(name, value, boundary) {
  return {
    benchmark: { uuid: `b-${name}`, name },
    measures: [{
      measure: latency, metric: { uuid: `metric-${name}`, value },
      boundary, threshold: boundary ? { model } : undefined,
    }],
  };
}

function fixture() {
  return {
    uuid: "r-1",
    project: { slug: "proj" },
    testbed: { uuid: "t-1", name: "intel-v1" },
    branch: {
      uuid: "br-pr", name: "pr-8",
      head: {
        uuid: "h-pr", version: { hash },
        start_point: { branch: "br-main", head: "h-main", version: { hash: baseHash } },
      },
    },
    results: [[
      result("rust/u64/random/49/s1/api", 736, { baseline: 709, lower_limit: 705, upper_limit: 713 }),
      result("rust/u64/sorted99/1000000/s1/api", 733100, { baseline: 1033988, lower_limit: 986115, upper_limit: 1081862 }),
      result("rust/u64/equal/65536/s1/api", 26000, { baseline: 26031, lower_limit: 23707, upper_limit: 28354 }),
      result("rust/u32/sorted99/65536/s1/api", 41000, null),
      result("rust/row_u32/random/48/s1/api", 800, { baseline: 726, lower_limit: 725, upper_limit: 727 }),
    ]],
    alerts: [
      { metric: { uuid: "metric-rust/u64/random/49/s1/api" }, limit: "upper", status: "active" },
      { metric: { uuid: "metric-rust/u64/sorted99/1000000/s1/api" }, limit: "lower", status: "active" },
      { metric: { uuid: "metric-rust/row_u32/random/48/s1/api" }, limit: "upper", status: "active" },
    ],
  };
}

test("publishes an SVG artifact for an alerted report", async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tessera-report-"));
  const previous = process.env.RUNNER_TEMP;
  try {
    process.env.RUNNER_TEMP = dir;
    const reportPath = path.join(dir, "report.json");
    fs.writeFileSync(reportPath, JSON.stringify(fixture()));
    const outputs = {};
    const core = {
      setOutput: (name, value) => { outputs[name] = value; },
      summary: { addRaw() { return this; }, async write() {} },
    };
    await report({ github: {}, context: { repo: { owner: "o", repo: "r" },
      serverUrl: "https://github.com", runId: 1 }, core,
    inputs: { language: "rust", hash, outcome: "success", reportPath } });
    assert.equal(outputs.chart, "true");
    assert.match(fs.readFileSync(path.join(dir, "benchmark-chart.svg"), "utf8"), /<svg/);
  } finally {
    if (previous === undefined) delete process.env.RUNNER_TEMP;
    else process.env.RUNNER_TEMP = previous;
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

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
    { type: "{u64,u32}", input: "random", n: "4,097", extra: "workspace" });
  assert.equal(report.caseLabel("rust/u32/d16/48/s2/api"), "u32 · 16 values · 48, seed 2");
  assert.equal(report.caseLabel("not/a/benchmark"), "not/a/benchmark");
});

test("leads with counts and call-outs, then the tables", () => {
  const body = report.render(report.summarize(fixture()),
    { language: "rust", runUrl: "https://run", baseRef: "main", chartRef: "./chart.svg" });
  assert.equal(body.split("\n")[0], "### Benchmarks · rust · 2 slower, 1 faster, 1 new");
  const caution = body.indexOf("> [!CAUTION]");
  const tip = body.indexOf("> [!TIP]");
  const chart = body.indexOf("](./chart.svg)");
  assert.ok(caution > 0 && caution < tip && tip < chart && chart < body.indexOf("#### Slower"));
  assert.match(body, /Worst: \{u32,u32\} · random · 48, 726 ns → \*\*800 ns\*\*, \*\*\+10\.2%\*\*/);
  assert.match(body, /1 case faster, 1 by 20% or more\.\*\* Best: u64 · 99% sorted · 1,000,000.*\*\*−29\.1%\*\*/);
  assert.match(body, /\*\*Overall run time −[\d.]+%\*\* against `main`, the geometric mean over 4 cases/);
  assert.match(body, /aaaaaaa against `main` bbbbbbb on Testbed `intel-v1`/);
  // Names link to history, with the benchmark ID as the link title.
  assert.match(body, /\| \[random <sub>u64 · 49<\/sub>\]\(https:\/\/bencher\.dev\/perf\/proj\?[^ ]+ "rust\/u64\/random\/49\/s1\/api"\) \| 709 ns \| 736 ns \| \+3\.8% \|/);
  // Both directions read as the change in run time; 20% or more is bold.
  assert.match(body, /\| \*\*−29\.1%\*\* \|/);
  assert.ok(!body.includes("/console/"), "links must not require a login");
  assert.match(body, /\[report JSON\]\(https:\/\/api\.bencher\.dev\/v0\/projects\/proj\/reports\/r-1\)/);
  assert.match(body, /\[history JSON of alerted cases\]\(https:\/\/api\.bencher\.dev\/v0\/projects\/proj\/perf\?branches=br-main%2Cbr-pr&/);
  const folded = body.slice(body.indexOf("<details>"));
  assert.match(folded, /All 5 cases/);
  assert.match(folded, /\| all equal <sub>u64 · 65,536<\/sub> \| 26\.0 µs \| 23\.7 µs – 28\.4 µs \|.*\| `rust\/u64\/equal\/65536\/s1\/api` \|/);
  // The visible part lists only alerts and new cases.
  assert.ok(!body.slice(0, body.indexOf("<details>")).includes("all equal"));
  assert.ok(!/[\u{1F300}-\u{1FAFF}\u2705\u26A0]/u.test(body), "no emoji");
});

test("omits the chart reference without an image", () => {
  const body = report.render(report.summarize(fixture()), { language: "rust", runUrl: "r" });
  assert.ok(!body.includes("!["));
});

test("draws the headline and one labelled mark per alerted case", () => {
  const svg = report.chartSvg(report.summarize(fixture()));
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
  assert.equal(report.chartSvg(report.summarize(quiet)), null);
});

test("reports a quiet run as such", () => {
  const r = fixture();
  r.alerts = [];
  const body = report.render(report.summarize(r), { language: "rust", runUrl: "r" });
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

function github(comments, headSha = hash) {
  const calls = [];
  return {
    calls,
    paginate: async () => comments,
    graphql: async (_query, vars) => calls.push(["minimize", vars.id]),
    rest: {
      pulls: { get: async () => ({ data: { state: "open", head: { sha: headSha } } }) },
      issues: {
        listComments: Symbol("list"),
        createComment: async args => calls.push(["create", args.body]),
        updateComment: async args => calls.push(["update", args.comment_id]),
      },
    },
  };
}

const bot = { login: "github-actions[bot]" };
const core = { notice: () => {} };
const repo = { owner: "o", repo: "r" };

test("posts a new report and minimizes earlier ones", async () => {
  const gh = github([
    { id: 1, node_id: "n1", user: bot, body: `${report.marker("rust", "c".repeat(40))}\nold` },
    { id: 2, node_id: "n2", user: bot, body: "<h2><a>Bencher Report</a></h2>" },
    { id: 3, node_id: "n3", user: { login: "someone" }, body: report.marker("rust", "d".repeat(40)) },
    { id: 4, node_id: "n4", user: bot, body: `${report.marker("cpp", "c".repeat(40))}\nother language` },
  ]);
  await report.publishComment({ github: gh, repo, number: 8, language: "rust", hash, body: "new", core });
  assert.deepEqual(gh.calls, [
    ["create", `${report.marker("rust", hash)}\nnew`],
    ["minimize", "n1"], ["minimize", "n2"],
  ]);
});

test("a rerun of the same head updates its report", async () => {
  const gh = github([{ id: 5, node_id: "n5", user: bot, body: `${report.marker("rust", hash)}\nold` }]);
  await report.publishComment({ github: gh, repo, number: 8, language: "rust", hash, body: "new", core });
  assert.deepEqual(gh.calls, [["update", 5]]);
});

test("does not comment for a superseded head", async () => {
  const gh = github([], "e".repeat(40));
  await report.publishComment({ github: gh, repo, number: 8, language: "rust", hash, body: "new", core });
  assert.deepEqual(gh.calls, []);
});
