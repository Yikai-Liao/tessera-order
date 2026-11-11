// SPDX-License-Identifier: BSD-2-Clause

import { mkdir, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { command } from "../../tools/process.ts";
import { continuous } from "../catalog.ts";
import { type PairedSource, pairedBmf, readPaired, validateCloud } from "./paired.ts";

export interface SubmissionOptions extends PairedSource {
  artifacts: string;
  prepared: string;
  project: string;
  out: string;
  pr?: string;
}

/** Upload validated JSON only; no executable, image, historical baseline or remote job.
 *
 * Cloud history stores the ratio to this run's exact baseline, not a
 * comparison with a previous run's ratio. The PR's reports share one branch
 * history, so merging main does not start a new one. A dedicated Testbed and
 * Measure isolate the old absolute-time histories.
 */
export async function submitContinuous(options: SubmissionOptions) {
  if (!/^[a-f0-9]{40}$/.test(options.hash) || !/^[a-f0-9]{40}$/.test(options.baseHash))
    throw new Error("Expected exact source and baseline SHAs");
  if (options.pr && !/^[1-9]\d*$/.test(options.pr))
    throw new Error("PR number must be a positive integer");
  if (!process.env.BENCHER_API_KEY) throw new Error("BENCHER_API_KEY is required for upload");
  await mkdir(options.out, { recursive: false });
  const runs = await readPaired(options.artifacts, options, options.prepared);
  const file = resolve(options.out, "bmf.json");
  await writeFile(file, `${JSON.stringify(pairedBmf(runs))}\n`);
  const report = await command(
    "bencher",
    [
      "run",
      "--project",
      options.project,
      "--branch",
      options.pr ? `pr-${options.pr}` : "main",
      "--hash",
      options.hash,
      "--testbed",
      continuous.testbed,
      "--adapter",
      "json",
      "--file",
      file,
      "--format",
      "json",
      "--quiet",
    ],
    { capture: true },
  );
  await writeFile(join(options.out, "bencher.json"), `${report}\n`);
  validateCloud(JSON.parse(report), runs, options.pr);
}
