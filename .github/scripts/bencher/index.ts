// SPDX-License-Identifier: BSD-2-Clause

import { readFileSync } from "node:fs";
import { chartSvg } from "./chart.ts";
import { loadHistory, summarizeTrend } from "./history.ts";
import { publishImages, type ChartImage, type ChartLinks } from "./images.ts";
import { publishComment, workflowUrl, type ActionContext, type ActionsCore, type GitHub } from "./github.ts";
import { headline, render, renderFailure } from "./markdown.ts";
import { summarize, type BencherReport, type ReportSummary } from "./report.ts";
import { trendSvg } from "./trend-chart.ts";

interface ReportInputs {
  language: string;
  hash: string;
  number?: string;
  baseRef?: string;
  checkId?: string;
  outcome: string;
  reportPath: string;
}

interface ReportOptions {
  github: GitHub;
  context: ActionContext;
  core: ActionsCore;
  inputs: ReportInputs;
  fetchHistory?: typeof fetch;
}

type Measurement = { ok: true; report: BencherReport; summary: ReportSummary } | { ok: false; reason: string };
type CompletedMeasurement = Extract<Measurement, { ok: true }>;

export async function publishReport({ github, context, core, inputs, fetchHistory }: ReportOptions) {
  const { language, hash, number, baseRef, checkId, outcome, reportPath } = inputs;
  const repo = context.repo;
  const runUrl = workflowUrl(context);
  const measurement = readMeasurement(outcome, reportPath);
  let body: string;
  if (measurement.ok) {
    const charts = await prepareCharts(measurement, inputs, core, fetchHistory);
    let links: ChartLinks = {};
    let imagesUnavailable = false;
    try {
      links = await publishImages({ github, context, language, hash, number, images: charts.images });
    } catch (error) {
      core.notice(`Benchmark image publication failed: ${errorMessage(error)}`);
      imagesUnavailable = true;
    }
    body = render(measurement.summary, {
      language, runUrl, baseRef, chartRef: links.comparison, trendRef: links.trend,
      trendUnavailable: charts.trendUnavailable, imagesUnavailable,
    });
  } else {
    body = renderFailure({ language, hash, runUrl, reason: measurement.reason });
  }
  await core.summary.addRaw(body).write();
  if (checkId) {
    await github.rest.checks.update({
      ...repo, check_run_id: Number(checkId), status: "completed",
      conclusion: outcome === "cancelled" ? "cancelled"
        : !measurement.ok || measurement.summary.regressions ? "failure" : "success",
      details_url: runUrl,
      output: {
        title: measurement.ok ? headline(measurement.summary) : "Measurement failed",
        summary: body.slice(0, 65000),
      },
    });
  }
  if (number && outcome !== "cancelled") {
    await publishComment({ github, repo, number: Number(number), language, hash, body, core });
  }
}

// Charts are supplemental. A history or image failure must not change the
// current measurement's verdict; the tables carry every number.
async function prepareCharts(measurement: CompletedMeasurement, inputs: ReportInputs, core: ActionsCore, fetchHistory?: typeof fetch) {
  const images: ChartImage[] = [];
  const comparison = chartSvg(measurement.summary);
  if (comparison) images.push({ kind: "comparison", svg: comparison });
  // The trend follows a PR branch; main has no PR history to draw.
  if (!inputs.number) return { images, trendUnavailable: false };
  try {
    const reports = await loadHistory(measurement.report, fetchHistory);
    const trend = summarizeTrend(reports, measurement.report, inputs.language);
    if (!trend) return { images, trendUnavailable: true };
    images.push({ kind: "trend", svg: trendSvg(trend) });
    return { images, trendUnavailable: false };
  } catch (error) {
    core.notice(`PR benchmark trend unavailable: ${errorMessage(error)}`);
    return { images, trendUnavailable: true };
  }
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function readMeasurement(outcome: string, reportPath: string): Measurement {
  if (outcome !== "success") return { ok: false, reason: `the Bencher job ${outcome}` };
  try {
    const report = JSON.parse(readFileSync(reportPath, "utf8")) as BencherReport;
    const summary = summarize(report);
    return summary.rows.length
      ? { ok: true, report, summary }
      : { ok: false, reason: "the report has no results" };
  } catch (error) {
    return { ok: false, reason: `the report could not be read (${errorMessage(error)})` };
  }
}
