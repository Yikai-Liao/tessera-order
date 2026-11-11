// SPDX-License-Identifier: BSD-2-Clause

import { readFile } from "node:fs/promises";
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
import { type ChartLinks, publishImages } from "./images.ts";
import {
  type ConfigurationResult,
  headline,
  render,
  renderCombined,
  renderFailure,
} from "./markdown.ts";
import { type PairedSource, pairedSummary, readPaired } from "./paired.ts";

interface PublicationOptions {
  github: GitHub;
  context: ActionContext;
  core: ActionsCore;
  inputs: PairedSource & {
    artifacts: string;
    inventory: string;
    reports: string;
    number?: string;
    baseRef?: string;
  };
}

/** Publish same-run comparisons from validated raw paired samples using trusted code.
 *
 * Only JSON is read from measurement artifacts. Missing shards and identity
 * mismatches fail their configuration; upload/image failures cannot turn absent
 * measurements into a pass or discard the other configuration's evidence.
 */
export async function publishReport({ github, context, core, inputs }: PublicationOptions) {
  const { hash, baseHash, number, baseRef = "main", artifacts } = inputs;
  const runUrl = workflowUrl(context);
  const measured = await Promise.all(
    VARIANTS.map(async (variant) => {
      const checkId = await startReport({ github, context, core, hash, variant });
      try {
        const expectedCases: string[] = JSON.parse(await readFile(inputs.inventory, "utf8"));
        const runs = await readPaired(artifacts, variant, { hash, baseHash }, expectedCases);
        return {
          variant,
          checkId,
          summary: pairedSummary(runs, variant),
          reason: undefined,
        };
      } catch (error) {
        return { variant, checkId, summary: null, reason: String(error) };
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
    links = await publishImages({
      github,
      context,
      language: "rust",
      hash,
      number,
      images: svg ? [{ kind: "comparison", svg }] : [],
    });
  } catch (error) {
    core.notice(`Benchmark images unavailable: ${String(error)}`);
    imagesUnavailable = true;
  }
  const note =
    "Adjacent A/B samples alternate AB/BA order. Median ratios use a distribution-free 99% median interval; an alert requires the entire interval outside 0.95–1.05. Coverage assumes independent samples, not immunity to code placement or interference. Each shard measures both revisions on one runner. Cloud archives results; it does not decide verdicts. Baseline changes split PR history; main points describe individual parent-to-commit changes, not absolute or cumulative performance.";
  const footer =
    "\n\nCloud archival runs separately after these checks. See the workflow for upload status; raw measurements remain in the Actions artifacts.";
  const options = {
    language: "rust",
    hash,
    runUrl,
    baseRef,
    chartRef: links.comparison,
    imagesUnavailable,
  };
  for (const result of measured) {
    const body = result.summary
      ? render(result.summary, { ...options, configuration: result.variant.name }) +
        "\n\n" +
        note +
        footer
      : renderFailure({ ...options, configuration: result.variant.name, reason: result.reason! });
    await github.rest.checks.update({
      ...context.repo,
      check_run_id: result.checkId,
      status: "completed",
      conclusion: !result.summary || result.summary.regressions ? "failure" : "success",
      details_url: runUrl,
      output: {
        title: result.summary ? headline(result.summary) : "Measurement failed",
        summary: body.slice(0, 65000),
      },
    });
  }
  const body = `${renderCombined(configurations, options)}\n\n${note}${footer}`;
  await core.summary.addRaw(body).write();
  if (number)
    await publishComment({
      github,
      repo: context.repo,
      number: Number(number),
      language: "rust",
      hash,
      body,
      core,
    });
}
