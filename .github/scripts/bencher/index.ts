// SPDX-License-Identifier: BSD-2-Clause

import { readFileSync, writeFileSync } from "node:fs";
import { chartSvg } from "./chart.ts";
import { publishComment, workflowUrl, type ActionContext, type ActionsCore, type GitHub } from "./github.ts";
import { headline, render, renderFailure } from "./markdown.ts";
import { summarize, type BencherReport, type ReportSummary } from "./report.ts";

interface ReportInputs {
  language: string;
  hash: string;
  number?: string;
  baseRef?: string;
  checkId?: string;
  outcome: string;
  reportPath: string;
  chartPath: string;
}

interface ReportOptions {
  github: GitHub;
  context: ActionContext;
  core: ActionsCore;
  inputs: ReportInputs;
}

export async function publishReport({ github, context, core, inputs }: ReportOptions) {
  const { language, hash, number, baseRef, checkId, outcome, reportPath, chartPath } = inputs;
  const repo = context.repo;
  const runUrl = workflowUrl(context);

  const measurement = readMeasurement(outcome, reportPath);
  const body = measurement.ok
    ? render(measurement.summary, { language, runUrl, baseRef })
    : renderFailure({ language, hash, runUrl, reason: measurement.reason });

  const chart = measurement.ok && chartSvg(measurement.summary);
  if (chart) {
    writeFileSync(chartPath, chart);
    core.setOutput("chart", "true");
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

type Measurement = { ok: true; summary: ReportSummary } | { ok: false; reason: string };

function readMeasurement(outcome: string, reportPath: string): Measurement {
  if (outcome !== "success") {
    return { ok: false, reason: `the Bencher job ${outcome}` };
  }
  try {
    const summary = summarize(JSON.parse(readFileSync(reportPath, "utf8")) as BencherReport);
    return summary.rows.length
      ? { ok: true, summary }
      : { ok: false, reason: "the report has no results" };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return { ok: false, reason: `the report could not be read (${message})` };
  }
}
