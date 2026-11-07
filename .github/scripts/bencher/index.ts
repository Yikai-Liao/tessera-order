// SPDX-License-Identifier: BSD-2-Clause

import { readFileSync } from "node:fs";
import { chartSvg } from "./chart.ts";
import { loadHistory, summarizeTrend } from "./history.ts";
import { publishImages, type ChartImage, type ChartLinks } from "./images.ts";
import { publishComment, workflowUrl, type ActionContext, type ActionsCore, type GitHub } from "./github.ts";
import { headline, render, renderFailure, renderOverview, type ConfigurationResult } from "./markdown.ts";
import { summarize, type BencherReport, type ReportSummary } from "./report.ts";
import { trendSvg } from "./trend-chart.ts";
import type { Variant } from "./variants.ts";

interface VariantInputs {
  variant: Variant;
  checkId?: string;
  outcome: string;
  reportPath: string;
}

interface ReportInputs {
  language: string;
  hash: string;
  number?: string;
  baseRef?: string;
  variants: VariantInputs[];
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

export async function publishReport(options: ReportOptions) {
  const { github, context, core, inputs } = options;
  const { language, hash, number, baseRef } = inputs;
  const runUrl = workflowUrl(context);
  const sections: string[] = [];
  const configurations: ConfigurationResult[] = [];
  for (const result of inputs.variants) {
    const { variant, checkId, outcome } = result;
    const measurement = readMeasurement(result, hash);
    configurations.push({ name: variant.name, outcome, summary: measurement.ok ? measurement.summary : null });
    let body: string;
    if (measurement.ok) {
      body = await renderMeasurement(measurement, variant, options);
    } else {
      body = renderFailure({ language: `${language} · ${variant.name}`, hash, runUrl, reason: measurement.reason });
    }
    sections.push(body);
    if (checkId) {
      await github.rest.checks.update({
        ...context.repo, check_run_id: Number(checkId), status: "completed",
        conclusion: outcome === "cancelled" ? "cancelled"
          : !measurement.ok || measurement.summary.regressions ? "failure" : "success",
        details_url: runUrl,
        output: {
          title: measurement.ok ? headline(measurement.summary) : "Measurement failed",
          summary: body.slice(0, 65000),
        },
      });
    }
  }
  const body = [renderOverview(language, baseRef || "main", configurations), ...sections].join("\n\n");
  await core.summary.addRaw(body).write();
  if (number && inputs.variants.some(result => result.outcome !== "cancelled")) {
    await publishComment({ github, repo: context.repo, number: Number(number), language, hash, body, core });
  }
}

async function renderMeasurement(measurement: CompletedMeasurement, variant: Variant, options: ReportOptions) {
  const { github, context, core, inputs, fetchHistory } = options;
  const { language, hash, number, baseRef } = inputs;
  const charts = await prepareCharts(measurement, inputs, core, fetchHistory);
  let links: ChartLinks = {};
  let imagesUnavailable = false;
  try {
    links = await publishImages({ github, context, language, variant: variant.id, hash, number, images: charts.images });
  } catch (error) {
    core.notice(`Benchmark image publication failed (${variant.id}): ${errorMessage(error)}`);
    imagesUnavailable = true;
  }
  return render(measurement.summary, {
    language: `${language} · ${variant.name}`, runUrl: workflowUrl(context), baseRef,
    chartRef: links.comparison, trendRef: links.trend,
    trendUnavailable: charts.trendUnavailable, imagesUnavailable,
  });
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

function readMeasurement({ outcome, reportPath, variant }: VariantInputs, hash: string): Measurement {
  if (outcome !== "success") return { ok: false, reason: `the Bencher job ${outcome}` };
  try {
    const report = JSON.parse(readFileSync(reportPath, "utf8")) as BencherReport;
    if (report.testbed?.name !== variant.testbed || report.branch?.head?.version?.hash !== hash) {
      return { ok: false, reason: "the report does not match the expected configuration and commit" };
    }
    const summary = summarize(report);
    return summary.rows.length
      ? { ok: true, report, summary }
      : { ok: false, reason: "the report has no results" };
  } catch (error) {
    return { ok: false, reason: `the report could not be read (${errorMessage(error)})` };
  }
}
