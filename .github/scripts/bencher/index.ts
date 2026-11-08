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
import { type BencherReport, type ReportSummary, short, summarize } from "./report.ts";
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
  // The Bencher branch and commit a PR run must compare against. A stacked
  // PR's start point is its parent's `pr-<number>`, not the base branch name.
  startPoint?: string;
  baseHash?: string;
  // Why the tracker could not name a start point, when it could not.
  baselineReason?: string;
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
  | {
      ok: true;
      report: BencherReport;
      summary: ReportSummary;
      // Why the promised comparison is absent although the cases were
      // measured; null when the comparison happened as intended.
      unavailable: string | null;
    }
  | { ok: false; reason: string };

// What a PR run promised to compare against, as the tracker resolved it.
interface Comparison {
  startPoint: string;
  hash: string | null;
  reason: string | null;
}

export async function publishReport(options: ReportOptions) {
  const { github, context, core, inputs, fetchHistory } = options;
  const { language, hash, number, baseRef } = inputs;
  const runUrl = workflowUrl(context);
  // Every PR run promises a comparison; `main` records samples without one.
  const baseline: Comparison | null = number
    ? {
        startPoint: inputs.startPoint || baseRef || "main",
        hash: inputs.baseHash || null,
        reason: inputs.baselineReason || null,
      }
    : null;
  const measured = inputs.variants.map((result) => ({
    ...result,
    measurement: readMeasurement(result, hash, baseline),
  }));
  const configurations: ConfigurationResult[] = measured.map(
    ({ variant, outcome, measurement }) => ({
      name: variant.name,
      outcome,
      summary: measurement.ok ? measurement.summary : null,
      reason: measurement.ok ? undefined : measurement.reason,
      unavailable: measurement.ok ? (measurement.unavailable ?? undefined) : undefined,
    }),
  );

  // One comparison and one history chart hold every configuration as a
  // column, so a build adds a column rather than another pair of images.
  const images: ChartImage[] = [];
  // Charts draw measured builds only; the overview names a failed one.
  const comparison = chartSvg(
    configurations.filter((c) => c.summary),
    baseRef,
  );
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
    const trend = trendSvg(
      trends.filter((c) => c.trend),
      baseRef,
    );
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
          unavailable: measurement.unavailable ?? undefined,
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
      // A promised comparison that did not happen is not a pass: a green
      // check must mean the cases were compared, not merely measured.
      conclusion:
        outcome === "cancelled"
          ? "cancelled"
          : !measurement.ok || measurement.summary.regressions || measurement.unavailable
            ? "failure"
            : "success",
      details_url: runUrl,
      output: {
        title: measurement.ok
          ? headline(measurement.summary, Boolean(measurement.unavailable))
          : "Measurement failed",
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
  baseline: Comparison | null,
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
    if (!summary.rows.length) return { ok: false, reason: "the report has no results" };
    return { ok: true, report, summary, unavailable: unavailableReason(report, summary, baseline) };
  } catch (error) {
    return { ok: false, reason: `the report could not be read (${errorMessage(error)})` };
  }
}

// Bencher records a start point only when it can resolve one, and it reports
// no error when it cannot. The tracker therefore states which comparison it
// promised and checks the report against it: a missing comparison is a failed
// check, not a passing one with every row marked new.
function unavailableReason(
  report: BencherReport,
  summary: ReportSummary,
  baseline: Comparison | null,
) {
  if (!baseline) return null;
  if (baseline.reason) return baseline.reason;
  if (!baseline.hash) return "the tracker did not resolve the base commit";
  const start = report.branch.head.start_point;
  if (!start) {
    return `Bencher recorded no start point; expected \`${baseline.startPoint}\` at ${short(baseline.hash)}`;
  }
  if (start.version?.hash !== baseline.hash) {
    return `the report compares against ${short(start.version?.hash ?? null)} instead of \`${baseline.startPoint}\` ${short(baseline.hash)}`;
  }
  return summary.compared
    ? null
    : `no case has a baseline at \`${baseline.startPoint}\` ${short(baseline.hash)}`;
}
