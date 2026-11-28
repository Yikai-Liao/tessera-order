// SPDX-License-Identifier: BSD-2-Clause

import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { codeChange } from "../code-change.ts";
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
import { headline, render, renderFailure } from "./markdown.ts";
import { type PairedSource, pairedSummary, readPaired, validateCloud } from "./paired.ts";
import type { BencherReport, ReportSummary } from "./report.ts";
import { trendSvg } from "./trend-chart.ts";

interface PublicationOptions {
  github: GitHub;
  context: ActionContext;
  core: ActionsCore;
  fetchHistory?: typeof fetch;
  readCodeChange?: typeof codeChange;
  inputs: PairedSource & {
    artifacts: string;
    prepared: string;
    reports?: string;
    publish?: boolean;
    language?: "rust" | "go";
    noBaseline?: boolean;
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
  readCodeChange = codeChange,
}: PublicationOptions) {
  const { hash, baseHash, number, baseRef = "main", artifacts } = inputs;
  const publish = inputs.publish !== false;
  const { language = "rust" } = inputs;
  const noBaseline = language === "go" && inputs.noBaseline;
  const runUrl = workflowUrl(context);
  const checkId = publish ? await startReport({ github, context, hash, language }) : undefined;
  let summary: ReportSummary | undefined;
  let trend: TrendSummary | null = null;
  let reason = "";
  if (!noBaseline)
    try {
      const runs = await readPaired(artifacts, { hash, baseHash, language }, inputs.prepared);
      let cloud: BencherReport | undefined;
      if (inputs.reports) {
        try {
          cloud = JSON.parse(await readFile(join(inputs.reports, "bencher.json"), "utf8"));
          validateCloud(cloud!, runs, number);
        } catch (error) {
          cloud = undefined;
          core.notice(`Cloud links unavailable: ${String(error)}`);
        }
      }
      summary = pairedSummary(runs, cloud);
      trend = summarizePairedTrend(summary, language);
      if (cloud) {
        try {
          trend =
            summarizePairedTrend(
              summary,
              language,
              cloud,
              await loadHistory(cloud, fetchHistory),
            ) ?? trend;
        } catch (error) {
          core.notice(`PR history unavailable: ${String(error)}`);
        }
      }
    } catch (error) {
      reason = String(error);
    }
  let code: Awaited<ReturnType<typeof codeChange>> | undefined;
  try {
    if (language === "rust") code = await readCodeChange(baseHash, hash);
  } catch (error) {
    core.notice(`Core engine line analysis unavailable: ${String(error)}`);
  }
  let links: ChartLinks = {};
  let imagesUnavailable = false;
  try {
    const svg = summary && chartSvg(summary, baseRef);
    const trendImage = trend && trendSvg(trend, baseRef);
    links = await publishImages({
      github,
      context,
      language,
      hash,
      number,
      images: publish
        ? [
            ...(svg ? [{ kind: "comparison" as const, svg }] : []),
            ...(trendImage ? [{ kind: "trend" as const, svg: trendImage }] : []),
          ]
        : [],
    });
  } catch (error) {
    core.notice(`Benchmark images unavailable: ${String(error)}`);
    imagesUnavailable = true;
  }
  const options = {
    language,
    hash,
    runUrl,
    baseRef,
    chartRef: links.comparison,
    trendRef: links.trend,
    imagesUnavailable,
    codeChange: code,
  };
  const body = noBaseline
    ? `### Benchmarks · ${language}\n\n**No comparable baseline**. The Go module is absent at \`${baseHash}\`. [Workflow run](${runUrl}).`
    : summary
      ? render(summary, options)
      : renderFailure({ ...options, reason });
  if (checkId !== undefined)
    await github.rest.checks.update({
      ...context.repo,
      check_run_id: checkId,
      status: "completed",
      conclusion: noBaseline ? "neutral" : !summary || summary.regressions ? "failure" : "success",
      details_url: runUrl,
      output: {
        title: noBaseline
          ? "No comparable baseline"
          : summary
            ? headline(summary)
            : "Results could not be validated",
        summary: body.slice(0, 65000),
      },
    });
  await core.summary.addRaw(body).write();
  if (number && publish)
    await publishComment({
      github,
      repo: context.repo,
      number: Number(number),
      language,
      hash,
      body,
      core,
    });
  if (noBaseline) return;
  if (!summary) throw new Error("Benchmark results could not be validated");
  if (!publish && summary.regressions)
    throw new Error("Benchmark regression alerts; see the job summary");
}
