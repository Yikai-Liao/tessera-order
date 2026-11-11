// SPDX-License-Identifier: BSD-2-Clause

import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { VARIANTS } from "../catalog.ts";
import { chartSvg } from "./chart.ts";
import {
  type ActionContext,
  type ActionsCore,
  type GitHub,
  publishComment,
  startReport,
  workflowUrl,
} from "./github.ts";
import { loadHistory, summarizePairedTrend, type TrendSummary } from "./history.ts";
import { type ChartLinks, publishImages } from "./images.ts";
import {
  type ConfigurationResult,
  headline,
  render,
  renderCombined,
  renderFailure,
} from "./markdown.ts";
import { type PairedSource, pairedSummary, readPaired, validateCloud } from "./paired.ts";
import type { BencherReport } from "./report.ts";
import { trendSvg } from "./trend-chart.ts";

interface PublicationOptions {
  github: GitHub;
  context: ActionContext;
  core: ActionsCore;
  fetchHistory?: typeof fetch;
  inputs: PairedSource & {
    artifacts: string;
    prepared: string;
    reports?: string;
    publish?: boolean;
    number?: string;
    baseRef?: string;
  };
}

/** Publish same-run comparisons from validated raw paired samples using trusted code.
 *
 * The branch’s prepare artifact owns selection and identity. Missing shards
 * fail validation; image and Cloud failures leave numeric evidence available.
 */
export async function publishReport({
  github,
  context,
  core,
  inputs,
  fetchHistory,
}: PublicationOptions) {
  const { hash, baseHash, number, baseRef = "main", artifacts } = inputs;
  const runUrl = workflowUrl(context);
  const measured = await Promise.all(
    VARIANTS.map(async (variant) => {
      const checkId =
        inputs.publish === false ? undefined : await startReport({ github, context, hash });
      try {
        const runs = await readPaired(artifacts, variant, { hash, baseHash }, inputs.prepared);
        let cloud: BencherReport | undefined;
        if (inputs.reports) {
          try {
            cloud = JSON.parse(
              await readFile(join(inputs.reports, `bencher-${variant.id}.json`), "utf8"),
            );
            validateCloud(cloud!, runs, variant, number);
          } catch (error) {
            cloud = undefined;
            core.notice(`Cloud links unavailable: ${String(error)}`);
          }
        }
        const summary = pairedSummary(runs, variant, cloud);
        let trend = summarizePairedTrend(summary, "rust");
        if (cloud) {
          try {
            trend =
              summarizePairedTrend(
                summary,
                "rust",
                cloud,
                await loadHistory(cloud, fetchHistory),
              ) ?? trend;
          } catch (error) {
            core.notice(`PR history unavailable: ${String(error)}`);
          }
        }
        return { variant, checkId, summary, trend, reason: undefined };
      } catch (error) {
        return {
          variant,
          checkId,
          summary: null,
          trend: null as TrendSummary | null,
          reason: String(error),
        };
      }
    }),
  );
  const configurations: ConfigurationResult[] = measured.map((result) => ({
    name: result.variant.name,
    outcome: result.summary ? "success" : "failure",
    summary: result.summary,
    reason: result.reason,
  }));
  let links: ChartLinks = {};
  let imagesUnavailable = false;
  try {
    const svg = chartSvg(configurations, baseRef);
    const trend = trendSvg(
      measured.map((result) => ({ name: result.variant.name, trend: result.trend })),
      baseRef,
    );
    links = await publishImages({
      github,
      context,
      language: "rust",
      hash,
      number,
      images:
        inputs.publish === false
          ? []
          : [
              ...(svg ? [{ kind: "comparison" as const, svg }] : []),
              ...(trend ? [{ kind: "trend" as const, svg: trend }] : []),
            ],
    });
  } catch (error) {
    core.notice(`Benchmark images unavailable: ${String(error)}`);
    imagesUnavailable = true;
  }
  const options = {
    language: "rust",
    hash,
    runUrl,
    baseRef,
    chartRef: links.comparison,
    trendRef: links.trend,
    imagesUnavailable,
  };
  for (const result of measured) {
    const body = result.summary
      ? render(result.summary, options)
      : renderFailure({ ...options, reason: result.reason! });
    if (result.checkId === undefined) continue;
    await github.rest.checks.update({
      ...context.repo,
      check_run_id: result.checkId,
      status: "completed",
      conclusion: !result.summary || result.summary.regressions ? "failure" : "success",
      details_url: runUrl,
      output: {
        title: result.summary ? headline(result.summary) : "Results could not be validated",
        summary: body.slice(0, 65000),
      },
    });
  }
  const body = renderCombined(configurations, options);
  await core.summary.addRaw(body).write();
  if (number && inputs.publish !== false)
    await publishComment({
      github,
      repo: context.repo,
      number: Number(number),
      language: "rust",
      hash,
      body,
      core,
    });
  if (measured.some((result) => !result.summary))
    throw new Error("Benchmark results could not be validated");
  if (inputs.publish === false && measured.some((result) => result.summary?.regressions))
    throw new Error("Benchmark regression alerts; see the job summary");
}
