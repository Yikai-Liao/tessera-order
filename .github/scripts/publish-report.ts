// SPDX-License-Identifier: BSD-2-Clause

import { appendFile } from "node:fs/promises";
import { join } from "node:path";
import { context, getOctokit } from "@actions/github";
import { reuseReport } from "../../bench/continuous/github.ts";
import { publishReport } from "../../bench/continuous/index.ts";

/** Adapt Actions environment and summary output to the shared publication operation. */
function required(name: string) {
  const value = process.env[name];
  if (!value) throw new Error(`Missing ${name}`);
  return value;
}

const reusedFrom = process.env.REUSED_FROM;
if (reusedFrom) {
  await reuseReport({
    github: getOctokit(required("GITHUB_TOKEN")),
    context,
    hash: required("HASH"),
    from: reusedFrom,
  });
  process.exit(0);
}
const publish = process.env.PUBLISH === "true";
const temporary = required("RUNNER_TEMP");
const summary = required("GITHUB_STEP_SUMMARY");
let body = "";
const output = {
  addRaw(text: string) {
    body += text;
    return output;
  },
  write: () => appendFile(summary, body),
};
await publishReport({
  github: getOctokit(publish ? required("GITHUB_TOKEN") : "read-only-report"),
  context,
  core: {
    notice: (message) => console.log(message),
    summary: output,
  },
  inputs: {
    hash: required("HASH"),
    baseHash: required("BASE_HASH"),
    number: process.env.PR_NUMBER,
    baseRef: process.env.BASE,
    artifacts: join(temporary, "paired"),
    prepared: join(temporary, "prepared"),
    reports: join(temporary, "submission"),
    publish,
  },
});
