// SPDX-License-Identifier: BSD-2-Clause

// Turns a `bencher run --format json` report into a PR comment, a check run
// and a job summary. Each measured PR head gets its own comment so the latest
// result sits at the end of the timeline; older reports are minimized as
// outdated, so a PR with many pushes shows one expanded report.

const BENCHER = "https://bencher.dev";
const API = "https://api.bencher.dev/v0";
const MARKER = "tessera-bench-report";

function short(hash) {
  return hash ? hash.slice(0, 7) : "unknown";
}

// Latency is in nanoseconds; three significant digits are enough for triage.
function formatTime(ns) {
  if (ns == null || !Number.isFinite(ns)) return "—";
  const [unit, scale] = ns >= 1e9 ? ["s", 1e9] : ns >= 1e6 ? ["ms", 1e6]
    : ns >= 1e3 ? ["µs", 1e3] : ["ns", 1];
  return `${(ns / scale).toPrecision(3)} ${unit}`;
}

function formatPercent(v) {
  const pct = Math.round(Math.abs(v) * 100);
  return v === 0 ? "0" : `${v > 0 ? "+" : "−"}${pct}%`;
}

function formatChange(change) {
  if (change == null || !Number.isFinite(change)) return "—";
  const pct = change * 100;
  return `${pct >= 0 ? "+" : "−"}${Math.abs(pct).toFixed(1)}%`;
}

const VERDICTS = {
  regression: { order: 0 },
  improvement: { order: 1 },
  missing: { order: 2 },
  within: { order: 3 },
};

// Display tier for alerted cases, chosen for readability rather than
// measured: the threshold model alone decides what is alerted, and this only
// decides which changes are set in bold.
const MAJOR = 0.2;

function isMajor(r) {
  return Math.abs(r.change ?? 0) >= MAJOR;
}

// Benchmark IDs are `<language>/<type>/<distribution>/<length>/<seed>/<mode>`,
// as in docs/development/benchmarking.md. Readers get the words; the ID stays
// in link titles and the full table, where it is copied into `cargo bench`.
const DISTRIBUTIONS = {
  random: "random", sorted: "sorted", reverse: "reverse", equal: "all equal",
  runs: "runs", d16: "16 values", d256: "256 values", "zipf1.1": "Zipf 1.1",
  zipf2: "Zipf 2", sorted95: "95% sorted", sorted99: "99% sorted",
  skew50: "50% one key", skew99: "99% one key", two_values: "two values",
  sparse8: "8 wide keys", sorted_tail: "sorted + tail",
};

function caseParts(id) {
  const parts = id.split("/");
  if (parts.length !== 6 || !/^\d+$/.test(parts[3])) return { type: "", input: id, n: "", extra: "" };
  const [, type, distribution, length, seed, mode] = parts;
  const row = type.match(/^row_(\w+)$/);
  // Seed 1 and the public API are the defaults; only departures are named.
  const extra = [];
  if (seed !== "s1") extra.push(`seed ${seed.slice(1)}`);
  if (mode !== "api") extra.push(mode);
  return {
    type: row ? `{${row[1]},u32}` : type,
    input: DISTRIBUTIONS[distribution] ?? distribution,
    n: Number(length).toLocaleString("en-US"),
    extra: extra.join(", "),
  };
}

function caseLabel(id) {
  const p = caseParts(id);
  return [p.type, p.input, p.n].filter(Boolean).join(" · ") + (p.extra ? `, ${p.extra}` : "");
}

// One row per benchmark and measure. Alerts are authoritative for the
// verdict; boundaries only supply the numbers shown beside it.
function summarize(report) {
  const alerts = new Map();
  for (const alert of report.alerts ?? []) {
    if (alert.status && alert.status !== "active") continue;
    // An alert names the metric it fired on; that UUID is unique per result.
    alerts.set(alert.metric.uuid, alert.limit);
  }
  const results = report.results?.[0] ?? [];
  const measureCount = new Set(
    results.flatMap(r => r.measures.map(m => m.measure.uuid)),
  ).size;
  const rows = [];
  for (const result of results) {
    for (const m of result.measures) {
      const value = m.metric?.value;
      const boundary = m.boundary ?? null;
      const baseline = boundary?.baseline ?? null;
      const limit = alerts.get(m.metric?.uuid);
      const verdict = limit === "upper" ? "regression"
        : limit === "lower" ? "improvement"
        : baseline == null ? "missing" : "within";
      const name = result.benchmark.name;
      rows.push({
        name, parts: caseParts(name), label: caseLabel(name) + (measureCount > 1 ? ` (${m.measure.name})` : ""), benchmark: result.benchmark.uuid, measure: m.measure.uuid,
        value, baseline, verdict,
        lower: boundary?.lower_limit ?? null,
        upper: boundary?.upper_limit ?? null,
        change: baseline ? value / baseline - 1 : null,
      });
    }
  }
  // Worst first: the largest slowdowns, then the largest speedups.
  rows.sort((a, b) =>
    VERDICTS[a.verdict].order - VERDICTS[b.verdict].order ||
    (a.verdict === "improvement" ? a.change - b.change : (b.change ?? 0) - (a.change ?? 0)) ||
    a.name.localeCompare(b.name));
  const count = verdict => rows.filter(r => r.verdict === verdict).length;
  const head = report.branch?.head;
  const model = results.flatMap(r => r.measures).find(m => m.threshold)?.threshold.model;
  // The geometric mean of current/baseline weighs a 2× win on a 700 ns case
  // and on a 10 ms case equally, which an arithmetic mean of times would not.
  const compared = rows.filter(r => r.baseline && r.value);
  const geomean = compared.length
    ? Math.exp(compared.reduce((sum, r) => sum + Math.log(r.value / r.baseline), 0) / compared.length)
    : null;
  return {
    rows,
    compared: compared.length,
    geomean,
    best: rows.find(r => r.verdict === "improvement") ?? null,
    worst: rows.find(r => r.verdict === "regression") ?? null,
    regressions: count("regression"),
    improvements: count("improvement"),
    missing: count("missing"),
    hash: head?.version?.hash ?? null,
    baseHash: head?.start_point?.version?.hash ?? null,
    branch: report.branch,
    testbed: report.testbed,
    project: report.project?.slug,
    uuid: report.uuid,
    model,
  };
}

function plural(n, word) {
  return `${n} ${word}${n === 1 ? "" : "s"}`;
}

// The headline leads with what the reader must act on, then what to
// celebrate. It is also the check title in the PR's check list.
function headline(s) {
  const parts = [];
  if (s.regressions) parts.push(`${s.regressions} slower`);
  if (s.improvements) parts.push(`${s.improvements} faster`);
  if (s.missing) parts.push(`${s.missing} new`);
  if (!s.regressions && !s.improvements) parts.push("no significant change");
  return parts.join(", ");
}

function caseLine(r) {
  return `${r.label}, ${formatTime(r.baseline)} → **${formatTime(r.value)}**`;
}

// GitHub alert blocks render as coloured boxes, so the verdict is visible
// before anyone reads a table. Regressions come first: they need action.
function callouts(s) {
  const out = [];
  const major = verdict => s.rows.filter(r => r.verdict === verdict && isMajor(r)).length;
  if (s.regressions) {
    const n = major("regression");
    out.push([
      "> [!CAUTION]",
      `> **${plural(s.regressions, "case")} slower${n ? `, ${n} by ${MAJOR * 100}% or more` : ""}.** ` +
        `Worst: ${caseLine(s.worst)}, **${formatChange(s.worst.change)}**.  `,
      "> Rerun, or confirm with the A/A and placement controls, before merging.",
    ].join("\n"));
  }
  if (s.improvements) {
    const n = major("improvement");
    out.push([
      "> [!TIP]",
      `> **${plural(s.improvements, "case")} faster${n ? `, ${n} by ${MAJOR * 100}% or more` : ""}.** ` +
        `Best: ${caseLine(s.best)}, **${formatChange(s.best.change)}**. Nice work.  `,
      "> Confirm with the full protocol before citing the speedup.",
    ].join("\n"));
  }
  return out.join("\n\n");
}

function overall(s, baseRef) {
  if (s.geomean == null) return null;
  const g = s.geomean;
  return `**Overall run time ${formatChange(g - 1)}** against \`${baseRef}\`, the geometric mean over ${s.compared} cases.`;
}

// Public perf pages need no login, unlike the console links in Bencher's own
// comment. A PR page plots the PR head beside its start point on main.
function perfUrl(s, benchmarks, base = `${BENCHER}/perf/${s.project}`) {
  const branches = [s.branch.uuid];
  const heads = [s.branch.head.uuid];
  const start = s.branch.head.start_point;
  if (start) {
    branches.unshift(start.branch);
    heads.unshift(start.head);
  }
  const query = new URLSearchParams({
    branches: branches.join(","), heads: heads.join(","),
    testbeds: s.testbed.uuid, benchmarks: benchmarks.join(","),
    measures: [...new Set(s.rows.map(r => r.measure))].join(","),
  });
  return `${base}?${query}`;
}

// The public API needs no login or token, so a reader or an agent can fetch
// the data directly; workflow artifacts would need a GitHub login and expire.
function dataLinks(s, alerted) {
  const api = `${API}/projects/${s.project}`;
  const links = [`[report JSON](${api}/reports/${s.uuid})`];
  if (alerted.length) {
    links.push(`[history JSON of alerted cases](${perfUrl(s, alerted.map(r => r.benchmark), `${api}/perf`)})`);
  }
  return `Data, without login: ${links.join(" · ")}. ` +
    "The report holds every case with its baseline and limits; the history holds each sample on `main` and this branch.";
}

// Every change is the signed change in run time: minus is less time.
function changeCell(r) {
  if (r.change == null) return "—";
  const text = formatChange(r.change);
  return isMajor(r) ? `**${text}**` : text;
}

// The case name links to its public history chart; the link title carries
// the benchmark ID for hover and for agents reading the Markdown.
// The input leads; type and n follow in small text, as in the chart.
function caseCell(r) {
  const rest = [secondary(r), r.parts.extra].filter(Boolean).join(" · ");
  return r.parts.input + (rest ? ` <sub>${rest}</sub>` : "");
}

function alertTable(rows, s) {
  const lines = ["| Case | Baseline | Current | Change |", "|---|---:|---:|---:|"];
  for (const r of rows) {
    lines.push(`| [${caseCell(r)}](${perfUrl(s, [r.benchmark])} "${r.name}") | ` +
      `${formatTime(r.baseline)} | ${formatTime(r.value)} | ${changeCell(r)} |`);
  }
  return lines.join("\n");
}

const STATUS = { regression: "slower", improvement: "faster", missing: "new", within: "" };

function fullTable(rows) {
  const lines = ["| Case | Baseline | Limits | Current | Change | Alert | ID |",
    "|---|---:|---:|---:|---:|---|---|"];
  for (const r of rows) {
    const limits = r.lower == null ? "—" : `${formatTime(r.lower)} – ${formatTime(r.upper)}`;
    lines.push(`| ${caseCell(r)} | ${formatTime(r.baseline)} | ${limits} | ${formatTime(r.value)} | ` +
      `${formatChange(r.change)} | ${STATUS[r.verdict]} | \`${r.name}\` |`);
  }
  return lines.join("\n");
}

function escapeXml(text) {
  return text.replace(/[&<>"]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
}

// Each side of the axis ends at its largest change, so no width goes to
// empty range.
function sideReach(rows) {
  return rows.length ? Math.max(...rows.map(r => Math.abs(r.change))) : 0;
}

// Tick step: the smallest round percentage whose ticks stay legible.
const STEPS = [0.05, 0.1, 0.2, 0.25, 0.5, 1, 2, 5];

function secondary(r) {
  return [r.parts.type, r.parts.n].filter(Boolean).join(" · ");
}

// Approximate advances of Inter in em, enough to lay out a label column
// without a font engine.
function textWidth(text, px) {
  let em = 0;
  for (const c of text) {
    em += /[il.,:;'·| ]/.test(c) ? 0.28 : /[{}()[\]frtj]/.test(c) ? 0.36
      : /[mwMW%×]/.test(c) ? 0.86 : /[0-9]/.test(c) ? 0.62 : /[A-Z]/.test(c) ? 0.68 : 0.55;
  }
  return em * px;
}

// The chart of the alerted cases, as a standalone SVG for an image
// attachment. It follows the documentation figures: neutral ink, hairline
// rules, tabular figures, and brand blue (faster) against orange (slower).
// The pair passes the colour-vision checks on the light and dark surfaces,
// and the media query swaps only the neutrals, so one file serves both themes.
function chartSvg(s) {
  const groups = [
    { key: "regression", title: "SLOWER", cls: "slower" },
    { key: "improvement", title: "FASTER", cls: "faster" },
  ].map(g => ({ ...g, rows: s.rows.filter(r => r.verdict === g.key).slice(0, 16) }))
    .filter(g => g.rows.length);
  if (!groups.length) return null;
  const all = groups.flatMap(g => g.rows);
  const slower = all.filter(r => r.verdict === "regression");
  const faster = all.filter(r => r.verdict === "improvement");

  // Type scale: headline 28, names 13, values 12, secondary 11, titles 10.5.
  const W = 720;
  const pad = 24;
  const rowH = 38;
  const tagW = r => (r.parts.extra ? textWidth(r.parts.extra, 10) + 12 : 0);
  const nameW = Math.max(...all.map(r => Math.max(
    textWidth(r.parts.input, 13) + (tagW(r) ? tagW(r) + 6 : 0),
    textWidth(secondary(r), 11))));
  const plotL = pad + Math.ceil(nameW) + 20;
  const plotR = W - pad;
  // One scale for both sides, each side only as long as its data, with room
  // beyond the outermost dot for its value.
  const room = 52;
  const rf = sideReach(faster);
  const rs = sideReach(slower);
  const leftRoom = faster.length ? room : 8;
  const rightRoom = slower.length ? room : 8;
  const k = (plotR - plotL - leftRoom - rightRoom) / (rf + rs);
  const zero = plotL + leftRoom + rf * k;
  const x = r => zero + r.change * k;

  const heroBottom = 76;
  const plotTop = heroBottom + 14;
  let y = plotTop;
  const layout = groups.map(g => {
    const titleY = y + 12;
    const first = y + 22;
    y = first + g.rows.length * rowH + 10;
    return { ...g, titleY, first };
  });
  const plotBottom = y - 6;
  const H = plotBottom + 44;

  const out = [];
  const t = (cls, xPos, yPos, text, extra = "") =>
    out.push(`<text class="${cls}" x="${xPos.toFixed(1)}" y="${yPos.toFixed(1)}"${extra}>${text}</text>`);
  out.push(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}" role="img" aria-labelledby="t d">`);
  out.push(`<title id="t">Benchmark changes: ${s.regressions} slower, ${s.improvements} faster</title>`);
  out.push(`<desc id="d">${escapeXml(all.map(r => `${r.label}: run time ${formatChange(r.change)}`).join("; "))}</desc>`);
  out.push(`<style>
  .panel { fill: #fafaf8; stroke: #d8d7d2; } .ring { stroke: #fafaf8; }
  .ink { fill: #0b0b0b; } .muted { fill: #6b6a66; } .rule { stroke: #e6e5e0; } .axis { stroke: #8d8c86; }
  .tag { fill: none; stroke: #c9c8c2; }
  .slower { fill: #e8622f; } .faster { fill: #4a80e6; }
  .stem-slower { stroke: #e8622f; } .stem-faster { stroke: #4a80e6; }
  text { font-family: Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif; font-size: 13px; font-feature-settings: "tnum", "cv11"; }
  .val { font-size: 12px; } .sub { font-size: 11px; } .tiny { font-size: 10px; } .strong { font-weight: 600; }
  .hero { font-size: 28px; font-weight: 600; letter-spacing: -0.6px; font-feature-settings: "pnum"; }
  .stat { font-size: 18px; font-weight: 600; }
  .caps { font-size: 10.5px; font-weight: 600; letter-spacing: 1.1px; }
  @media (prefers-color-scheme: dark) {
    .panel { fill: #1e1e1d; stroke: #3a3a37; } .ring { stroke: #1e1e1d; }
    .ink { fill: #e8e7e3; } .muted { fill: #a3a29b; } .rule { stroke: #30302e; } .axis { stroke: #6f6e69; }
    .tag { stroke: #4a4a46; }
  }
</style>`);
  out.push(`<rect class="panel" x="0.5" y="0.5" width="${W - 1}" height="${H - 1}" rx="12"/>`);

  // Headline: the overall ratio, then the counts.
  if (s.geomean != null) {
    const g = s.geomean;
    const dir = g < 1 ? "faster" : g > 1 ? "slower" : "ink";
    const pct = formatChange(g - 1);
    t(`hero ${dir}`, pad, 50, pct);
    const lx = pad + textWidth(pct, 28) + 14;
    t("ink", lx, 36, "run time overall");
    t("muted sub", lx, 51, `geometric mean of ${s.compared} cases`);
  }
  const stats = [[s.improvements, "faster", "faster"], [s.regressions, "slower", "slower"], [s.missing, "new", "ink"]]
    .filter(([n]) => n);
  let sx = plotR;
  for (const [n, word, cls] of [...stats].reverse()) {
    t("muted sub", sx, 50, word, ' text-anchor="end"');
    sx -= textWidth(word, 11) + 5;
    t(`stat ${cls}`, sx, 50, String(n), ' text-anchor="end"');
    sx -= textWidth(String(n), 18) + 22;
  }
  out.push(`<line class="rule" x1="${pad}" x2="${plotR}" y1="${heroBottom}" y2="${heroBottom}" stroke-width="1"/>`);

  // Rules at a round step on each side, at least 60 px apart so the grid
  // stays quiet.
  const step = STEPS.find(v => v * k >= 60) ?? STEPS[STEPS.length - 1];
  for (const [side, reach] of [[-1, rf], [1, rs]]) {
    for (let v = step; v <= reach + 1e-9; v += step) {
      const gx = zero + side * v * k;
      out.push(`<line class="rule" x1="${gx.toFixed(1)}" x2="${gx.toFixed(1)}" y1="${plotTop}" y2="${plotBottom}" stroke-width="1"/>`);
      t("muted tiny", gx, plotBottom + 16, formatPercent(side * v), ' text-anchor="middle"');
    }
  }
  out.push(`<line class="axis" x1="${zero.toFixed(1)}" x2="${zero.toFixed(1)}" y1="${plotTop}" y2="${plotBottom}" stroke-width="1"/>`);
  t("muted tiny", zero, plotBottom + 16, "0", ' text-anchor="middle"');
  t("faster sub", zero - 8, plotBottom + 33, "← less time", ' text-anchor="end"');
  t("slower sub", zero + 8, plotBottom + 33, "more time →");

  for (const g of layout) {
    t(`caps ${g.cls}`, pad, g.titleY, g.title);
    g.rows.forEach((r, i) => {
      const cy = g.first + i * rowH + rowH / 2;
      const strong = isMajor(r) ? " strong" : "";
      t(`ink${strong}`, pad, cy - 2, escapeXml(r.parts.input));
      if (r.parts.extra) {
        const tx = pad + textWidth(r.parts.input, 13) + 6;
        out.push(`<rect class="tag" x="${tx.toFixed(1)}" y="${cy - 13}" width="${tagW(r).toFixed(1)}" height="14" rx="7" stroke-width="1"/>`);
        t("muted tiny", tx + tagW(r) / 2, cy - 3, escapeXml(r.parts.extra), ' text-anchor="middle"');
      }
      t("muted sub", pad, cy + 13, escapeXml(secondary(r)));
      const dx = x(r);
      out.push(`<line class="stem-${g.cls}" x1="${zero.toFixed(1)}" x2="${dx.toFixed(1)}" y1="${cy}" y2="${cy}" stroke-width="2" stroke-linecap="round"/>`);
      out.push(`<circle class="${g.cls} ring" cx="${dx.toFixed(1)}" cy="${cy}" r="${isMajor(r) ? 5.5 : 4}" stroke-width="2"/>`);
      const left = r.verdict === "improvement";
      t(`val ink${strong}`, dx + (left ? -10 : 10), cy + 4, formatChange(r.change),
        ` text-anchor="${left ? "end" : "start"}"`);
    });
  }
  out.push("</svg>");
  return out.join("\n");
}

// `chartRef` is where the chart image will be, such as a local path that
// `gh --attach` rewrites to the uploaded asset. Without one the tables stand
// alone; they hold everything the chart shows.
function render(s, { language, runUrl, baseRef = "main", chartRef = null }) {
  const slower = s.rows.filter(r => r.verdict === "regression");
  const faster = s.rows.filter(r => r.verdict === "improvement");
  const missing = s.rows.filter(r => r.verdict === "missing");
  const base = s.baseHash ? `\`${baseRef}\` ${short(s.baseHash)}` : "recent history";
  const out = [`### Benchmarks · ${language} · ${headline(s)}`];
  const boxes = callouts(s);
  if (boxes) out.push("", boxes);
  if (chartRef && (slower.length || faster.length)) {
    out.push("", `![Change from baseline for each alerted case](${chartRef})`);
  }
  const summaryLine = overall(s, baseRef);
  out.push("", [summaryLine,
    `${short(s.hash)} against ${base} on Testbed \`${s.testbed.name}\` · ` +
      `[workflow run](${runUrl}) · [Bencher report](${BENCHER}/perf/${s.project}/reports/${s.uuid})`,
  ].filter(Boolean).join("  \n"));
  if (slower.length) out.push("", `#### Slower`, "", alertTable(slower, s));
  if (faster.length) out.push("", `#### Faster`, "", alertTable(faster, s));
  if (missing.length) {
    out.push("", `New, without a baseline on \`${baseRef}\`: ` +
      missing.map(r => `${r.label} (${formatTime(r.value)})`).join(", ") + ".");
  }
  out.push("", dataLinks(s, [...slower, ...faster]));
  out.push("", `<details><summary>All ${s.rows.length} cases</summary>`, "", fullTable(s.rows), "", "</details>");
  const m = s.model;
  const method = m
    ? `Baseline: the mean of up to ${m.max_sample_size} samples on \`${baseRef}\`, at least ${m.min_sample_size}. ` +
      `Limits: a ${m.test.replace("_", "-")} at ${m.lower_boundary}.`
    : "No threshold applies to this branch yet.";
  out.push("", `<sub>${method} An alert marks a case to investigate with the ` +
    "[benchmark protocol](https://github.com/Yikai-Liao/tessera-sort/blob/main/docs/development/benchmarking.md); " +
    "it is not evidence for a performance claim. Earlier reports on this PR are collapsed as outdated.</sub>");
  return out.join("\n");
}

function renderFailure({ language, hash, runUrl, reason }) {
  return [
    `### Benchmarks · ${language} · measurement failed`,
    "",
    `**${short(hash)}** was not measured: ${reason}. See the [workflow run](${runUrl}).`,
  ].join("\n");
}

function marker(language, hash) {
  return `<!-- ${MARKER} language=${language} sha=${hash} -->`;
}

function ownComment(comment, language) {
  return comment.user?.login === "github-actions[bot]" &&
    (comment.body?.includes(`<!-- ${MARKER} language=${language} `) ||
      // Comments written by `bencher run --github-actions` before this report.
      comment.body?.includes("Bencher Report</a></h2>"));
}

async function publishComment({ github, repo, number, language, hash, body, core }) {
  const { data: pr } = await github.rest.pulls.get({ ...repo, pull_number: number });
  if (pr.state !== "open" || pr.head.sha !== hash) {
    core.notice(`PR #${number} moved past ${short(hash)}; the report stays in the job summary.`);
    return;
  }
  const comments = (await github.paginate(github.rest.issues.listComments,
    { ...repo, issue_number: number, per_page: 100 })).filter(c => ownComment(c, language));
  const tag = marker(language, hash);
  const full = `${tag}\n${body}`;
  // A rerun of the same head replaces its report instead of adding one.
  const same = comments.find(c => c.body.startsWith(tag));
  if (same) {
    await github.rest.issues.updateComment({ ...repo, comment_id: same.id, body: full });
  } else {
    await github.rest.issues.createComment({ ...repo, issue_number: number, body: full });
  }
  for (const c of comments) {
    if (c === same) continue;
    await github.graphql(
      "mutation($id: ID!) { minimizeComment(input: {subjectId: $id, classifier: OUTDATED}) { clientMutationId } }",
      { id: c.node_id });
  }
}

// Entry point for actions/github-script.
module.exports = async ({ github, context, core, inputs }) => {
  const fs = require("node:fs");
  const { language, hash, number, baseRef, checkId, outcome, reportPath } = inputs;
  const repo = context.repo;
  const runUrl = `${context.serverUrl}/${repo.owner}/${repo.repo}/actions/runs/${context.runId}`;

  let summary = null;
  let reason = outcome === "success" ? null : `the Bencher job ${outcome}`;
  if (!reason) {
    try {
      summary = summarize(JSON.parse(fs.readFileSync(reportPath, "utf8")));
      if (!summary.rows.length) reason = "the report has no results";
    } catch (error) {
      reason = `the report could not be read (${error.message})`;
    }
  }
  const body = reason
    ? renderFailure({ language, hash, runUrl, reason })
    : render(summary, { language, runUrl, baseRef });

  const chart = summary && chartSvg(summary);
  if (chart) {
    fs.writeFileSync(`${process.env.RUNNER_TEMP}/benchmark-chart.svg`, chart);
    core.setOutput("chart", "true");
  }
  await core.summary.addRaw(body).write();
  if (checkId) {
    await github.rest.checks.update({
      ...repo, check_run_id: Number(checkId), status: "completed",
      conclusion: outcome === "cancelled" ? "cancelled"
        : reason || summary.regressions ? "failure" : "success",
      details_url: runUrl,
      output: {
        title: reason ? "Measurement failed" : headline(summary),
        // Check output is limited to 65,535 characters.
        summary: body.slice(0, 65000),
      },
    });
  }
  if (number && outcome !== "cancelled") {
    await publishComment({ github, repo, number: Number(number), language, hash, body, core });
  }
};

Object.assign(module.exports, { summarize, render, chartSvg, caseLabel, caseParts, renderFailure, formatTime, formatChange, ownComment, publishComment, marker });
