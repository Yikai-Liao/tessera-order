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

/** Publish same-run comparisons from validated raw blocks using trusted code.
 *
 * Only JSON is read from measurement artifacts. Missing shards and identity
 * mismatches fail their configuration; upload/image failures cannot turn absent
 * measurements into a pass or discard the other configuration's evidence.
 */
export async function publishReport({ github, context, core, inputs }: PublicationOptions) {
  const { hash, baseHash, number, baseRef = "main", artifacts, reports } = inputs;
  const runUrl = workflowUrl(context);
  const measured = await Promise.all(
    VARIANTS.map(async (variant) => {
      const checkId = await startReport({ github, context, core, hash, variant });
      try {
        const expectedCases: string[] = JSON.parse(await readFile(inputs.inventory, "utf8"));
        const runs = await readPaired(artifacts, variant, { hash, baseHash }, expectedCases);
        let cloud: BencherReport | undefined;
        try {
          cloud = JSON.parse(await readFile(join(reports, `bencher-${variant.id}.json`), "utf8"));
          validateCloud(cloud!, runs, variant, number);
        } catch (error) {
          cloud = undefined;
          core.notice(`Cloud upload unavailable (${variant.id}): ${String(error)}`);
        }
        return {
          variant,
          checkId,
          cloud,
          summary: pairedSummary(runs, variant, cloud),
          reason: undefined,
        };
      } catch (error) {
        return { variant, checkId, cloud: undefined, summary: null, reason: String(error) };
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
    "Nine interleaved process blocks on each runner. Current / mean of two baseline calls, then median across blocks; a direction requires eight agreeing blocks and a median outside the A/A range. Ranges are observed, not confidence intervals. Each shard measures both revisions on its own runner. Cloud stores these ratios without comparing different runs' baselines.";
  const uploadFailures = measured.filter((result) => result.summary && !result.cloud);
  const footer = uploadFailures.length
    ? "\n\n> Cloud upload failed: " +
      uploadFailures.map((result) => result.variant.name).join(", ") +
      ". Measurements remain in the Actions artifacts."
    : "";
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
      conclusion:
        !result.summary || !result.cloud || result.summary.regressions ? "failure" : "success",
      details_url: runUrl,
      output: {
        title: result.summary
          ? (!result.cloud ? "Cloud upload failed · " : "") + headline(result.summary)
          : "Measurement failed",
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
