// SPDX-License-Identifier: BSD-2-Clause

// Queued feedback and terminal cleanup outside the exclusive measurement slot.

import type { Variant } from "../catalog.ts";
import { type ActionArguments, publishComment, resolveSource, workflowUrl } from "./github.ts";
import { renderProgress } from "./markdown.ts";
import { checkName, latestCheck } from "./marker.ts";

type ReportArguments = Pick<ActionArguments, "github" | "context"> & {
  hash: string;
  variant: Variant;
};

/** Create a visible queued check and return its ID for the caller’s job outputs. */
export async function queueReport({ github, context, hash, variant }: ReportArguments) {
  const { data } = await github.rest.checks.create({
    ...context.repo,
    name: checkName(variant),
    head_sha: hash,
    external_id: String(context.runId),
    status: "queued",
    details_url: workflowUrl(context),
    output: {
      title: "Queued for measurement",
      summary: renderProgress({ hash, runUrl: workflowUrl(context), state: "queued" }),
    },
  });
  return String(data.id);
}

/** Start this tracker’s check, creating a fresh check on failed-job retries.
 * Completed verdicts are retained; the returned ID belongs to this measurement.
 */
export async function activateReport({
  github,
  context,
  hash,
  variant,
  checkId,
}: ReportArguments & { checkId: string }) {
  let id = Number(checkId);
  if (!Number.isSafeInteger(id) || id < 1) throw new Error("Invalid report check ID");
  const { data } = await github.rest.checks.get({ ...context.repo, check_run_id: id });
  if (
    data.head_sha !== hash ||
    data.external_id !== String(context.runId) ||
    data.name !== checkName(variant)
  )
    throw new Error("Report check does not belong to this source and tracker");
  if (data.status === "completed")
    id = Number(await queueReport({ github, context, hash, variant }));
  await github.rest.checks.update({
    ...context.repo,
    check_run_id: id,
    status: "in_progress",
    output: {
      title: "Preparing measurement",
      summary: renderProgress({ hash, runUrl: workflowUrl(context), state: "running" }),
    },
  });
  return String(id);
}

/** Replace stale visible feedback with the current source’s non-verdict state. */
export async function publishProgress(
  args: ActionArguments & {
    hash: string;
    number?: string;
    state: "queued" | "running";
  },
) {
  if (!args.number) return;
  await publishComment({
    github: args.github,
    repo: args.context.repo,
    core: args.core,
    number: Number(args.number),
    language: "rust",
    hash: args.hash,
    body: renderProgress({ hash: args.hash, runUrl: workflowUrl(args.context), state: args.state }),
  });
}

/** Close this tracker’s unfinished checks, including checks whose create reply was lost.
 * Completed measurement verdicts are never overwritten.
 */
export async function finishPendingReports(
  args: ActionArguments & {
    hash: string;
    number?: string;
    variants: readonly Variant[];
    conclusion: "failure" | "cancelled";
    reason: string;
  },
) {
  const { github, context, hash, variants, conclusion, reason } = args;
  const names = new Set(variants.map(checkName));
  const checks = await github.paginate(github.rest.checks.listForRef, {
    ...context.repo,
    ref: hash,
    filter: "all",
    per_page: 100,
  });
  const pending = checks.filter(
    (check) =>
      check.head_sha === hash &&
      check.external_id === String(context.runId) &&
      names.has(check.name) &&
      check.status !== "completed",
  );
  if (!pending.length) return;
  const body = renderProgress({ hash, runUrl: workflowUrl(context), state: "interrupted", reason });
  const outcomes = await Promise.allSettled(
    pending.map((check) =>
      github.rest.checks.update({
        ...context.repo,
        check_run_id: check.id,
        status: "completed",
        conclusion,
        output: { title: "Benchmark feedback interrupted", summary: body },
      }),
    ),
  );
  const failures = outcomes.filter((outcome) => outcome.status === "rejected");
  if (failures.length)
    throw new AggregateError(
      failures.map((failure) => failure.reason),
      "Could not close all tracking checks",
    );
  // Orphaned checks from an older attempt still need closing, but must not
  // replace a newer attempt’s progress or already-completed report.
  if (
    !variants.some((variant) => {
      const latest = latestCheck(checks, variant);
      return latest?.external_id === String(context.runId) && latest.status !== "completed";
    })
  )
    return;
  let number = args.number;
  if (!number) {
    // Recover source outputs if setup stopped after writing a check but before
    // handing its outputs to the dependent job. The same resolver owns PR identity.
    try {
      await resolveSource({
        ...args,
        core: {
          ...args.core,
          setOutput: (name, value) => {
            if (name === "number") number = value;
          },
        },
      });
    } catch {
      args.core.notice("Unfinished checks closed; PR identity was unavailable for a comment.");
    }
  }
  if (number)
    await publishComment({
      github,
      repo: context.repo,
      core: args.core,
      number: Number(number),
      language: "rust",
      hash,
      body,
    });
}
