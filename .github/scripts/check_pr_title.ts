// SPDX-License-Identifier: BSD-2-Clause

import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const types = ["feat", "fix", "perf", "refactor", "docs", "test", "ci", "build", "chore", "revert"];

/** Check the title that becomes the squash commit, leaving space for its PR number. */
export function titleErrors(title: string): string[] {
  const match = /^([a-z]+)(?:\(([a-z][a-z0-9-]*)\))?!?: (\S[^\r\n]*)$/.exec(title);
  if (!match || match[0] !== title)
    return ["expected `type(scope)!: summary`, with a lowercase scope and `!` optional"];
  const errors: string[] = [];
  if (!types.includes(match[1]))
    errors.push(`type \`${match[1]}\` is not one of ${types.join(", ")}`);
  if (title.endsWith(".")) errors.push("the summary ends with a period");
  const len = [...title].length;
  if (len > 64) errors.push(`${len} characters, over 64`);
  return errors;
}

/** Report title failures for both CI annotations and machine-readable CLI output. */
export function runTitleCheck(title: string) {
  const errors = titleErrors(title);
  console.log(JSON.stringify({ errors }));
  for (const error of errors) console.error(`::error::${error}`);
  if (errors.length) {
    console.error("Use the commit-message conventions in CONTRIBUTING.md.");
    process.exitCode = 1;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href)
  runTitleCheck(process.env.PR_TITLE ?? "");
