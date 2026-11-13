// SPDX-License-Identifier: BSD-2-Clause

import { readFile } from "node:fs/promises";

/** Remote failures with a known cause, and what to do about them. */
const KNOWN: [RegExp, (match: RegExpMatchArray) => string][] = [
  [
    /VM execution timed out after (\d+)s/,
    ([, seconds]) =>
      `A remote job ran longer than Bencher's ${seconds} s limit. bench/tracking/jobs.ts plans each job's share of that limit; a slow algorithm may need a smaller share.`,
  ],
  [
    /Timed out waiting for remote job after (\d+) seconds/,
    ([, seconds]) =>
      `Bencher did not finish a job within ${seconds} s of waiting, queueing included. Other runs hold the single remote slot; rerun this algorithm when the queue is shorter. The job may still complete in Bencher.`,
  ],
];

/** The known cause in a log, if any. */
export function explainFailure(log: string) {
  for (const [pattern, explain] of KNOWN) {
    const match = log.match(pattern);
    if (match) return explain(match);
  }
  return null;
}

const LOG_TAIL_LINES = 40;

/**
 * Run a tracking stage; on failure, state the cause once instead of a stack trace.
 *
 * Actions shows the cause as an annotation and the log's tail in a folded group;
 * the full log stays in the uploaded evidence.
 */
export async function reportFailure(title: string, log: string, stage: () => Promise<unknown>) {
  try {
    await stage();
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    const text = await readFile(log, "utf8").catch(() => "");
    const cause = explainFailure(text) ?? message;
    const actions = process.env.GITHUB_ACTIONS === "true";
    const escape = (value: string) =>
      value.replaceAll("%", "%25").replaceAll("\r", "%0D").replaceAll("\n", "%0A");
    console.error(
      actions ? `::error title=${escape(title)}::${escape(cause)}` : `${title}: ${cause}`,
    );
    if (cause !== message) console.error(message);
    const tail = text.trimEnd().split("\n").slice(-LOG_TAIL_LINES).join("\n");
    if (tail) {
      if (actions) console.error("::group::Last log lines");
      console.error(tail);
      if (actions) console.error("::endgroup::");
    }
    process.exitCode = 1;
  }
}
