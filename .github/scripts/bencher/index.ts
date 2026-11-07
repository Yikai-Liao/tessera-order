// SPDX-License-Identifier: BSD-2-Clause

import { readFileSync } from "node:fs";
import { chartSvg } from "./chart.ts";
import {
  type ActionContext,
  type ActionsCore,
  type GitHub,
  publishComment,
  workflowUrl,
} from "./github.ts";
import { loadHistory, summarizeTrend } from "./history.ts";
import { type ChartImage, type ChartLinks, publishImages } from "./images.ts";
import {
  type ConfigurationResult,
  headline,
  render,
  renderCombined,
  renderFailure,
} from "./markdown.ts";
import { type BencherReport, type ReportSummary, summarize } from "./report.ts";
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

type Measurement =
  | { ok: true; report: BencherReport; summary: ReportSummary }
  | { ok: false; reason: string };

export async function publishReport(options: ReportOptions) {
  const { github, context, core, inputs, fetchHistory } = options;
  const { language, hash, number, baseRef } = inputs;
  const runUrl = workflowUrl(context);
  const measured = inputs.variants.map((result) => ({
    ...result,
    measurement: readMeasurement(result, hash),
  }));
  const configurations: ConfigurationResult[] = measured.map(
    ({ variant, outcome, measurement }) => ({
      name: variant.name,
      outcome,
      summary: measurement.ok ? measurement.summary : null,
      reason: measurement.ok ? undefined : measurement.reason,
    }),
  );

  // One comparison and one history chart hold every configuration as a
  // column, so a build adds a column rather than another pair of images.
  const images: ChartImage[] = [];
  // Charts draw measured builds only; the overview names a failed one.
  const comparison = chartSvg(configurations.filter((c) => c.summary));
  if (comparison) images.push({ kind: "comparison", svg: comparison });
  let trendUnavailable = false;
  // The trend follows a PR branch; main has no PR history to draw.
  if (number && configurations.some((c) => c.summary)) {
    const trends = await Promise.all(
      measured.map(async ({ variant, measurement }) => {
        if (!measurement.ok) return { name: variant.name, trend: null };
        try {
          const reports = await loadHistory(measurement.report, fetchHistory);
          const trend = summarizeTrend(reports, measurement.report, language);
          if (!trend) trendUnavailable = true;
          return { name: variant.name, trend };
        } catch (error) {
          core.notice(`PR benchmark trend unavailable (${variant.id}): ${errorMessage(error)}`);
          trendUnavailable = true;
          return { name: variant.name, trend: null };
        }
      }),
    );
    const trend = trendSvg(trends.filter((c) => c.trend));
    if (trend) images.push({ kind: "trend", svg: trend });
  }
  // Charts are supplemental. A history or image failure must not change the
  // current measurement's verdict; the tables carry every number.
  let links: ChartLinks = {};
  let imagesUnavailable = false;
  try {
    links = await publishImages({ github, context, language, hash, number, images });
  } catch (error) {
    core.notice(`Benchmark image publication failed: ${errorMessage(error)}`);
    imagesUnavailable = true;
  }
  const charts = {
    chartRef: links.comparison,
    trendRef: links.trend,
    trendUnavailable,
    imagesUnavailable,
  };

  // Each configuration keeps its own check, so each can be required; the
  // check stands alone and reads like the comment for one build.
  for (const { variant, checkId, outcome, measurement } of measured) {
    if (!checkId) continue;
    const summary = measurement.ok
      ? render(measurement.summary, {
          language,
          configuration: variant.name,
          runUrl,
          baseRef,
          ...charts,
        })
      : renderFailure({
          language,
          configuration: variant.name,
          hash,
          runUrl,
          reason: measurement.reason,
        });
    await github.rest.checks.update({
      ...context.repo,
      check_run_id: Number(checkId),
      status: "completed",
      conclusion:
        outcome === "cancelled"
          ? "cancelled"
          : !measurement.ok || measurement.summary.regressions
            ? "failure"
            : "success",
      details_url: runUrl,
      output: {
        title: measurement.ok ? headline(measurement.summary) : "Measurement failed",
        summary: summary.slice(0, 65000),
      },
    });
  }

  const body = renderCombined(configurations, { language, hash, runUrl, baseRef, ...charts });
  await core.summary.addRaw(body).write();
  if (number && inputs.variants.some((result) => result.outcome !== "cancelled")) {
    await publishComment({
      github,
      repo: context.repo,
      number: Number(number),
      language,
      hash,
      body,
      core,
    });
  }
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function readMeasurement(
  { outcome, reportPath, variant }: VariantInputs,
  hash: string,
): Measurement {
  if (outcome !== "success") {
    return {
      ok: false,
      reason: `the Bencher job ${outcome === "failure" ? "failed" : `ended as ${outcome}`}`,
    };
  }
  try {
    const report = JSON.parse(readFileSync(reportPath, "utf8")) as BencherReport;
    if (report.testbed?.name !== variant.testbed || report.branch?.head?.version?.hash !== hash) {
      return {
        ok: false,
        reason: "the report does not match the expected configuration and commit",
      };
    }
    const summary = summarize(report);
    return summary.rows.length
      ? { ok: true, report, summary }
      : { ok: false, reason: "the report has no results" };
  } catch (error) {
    return { ok: false, reason: `the report could not be read (${errorMessage(error)})` };
  }
}
