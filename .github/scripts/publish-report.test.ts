// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { cp, mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { writePaired } from "../../bench/continuous/fixtures/paired.ts";
import { baseHash, hash } from "../../bench/continuous/fixtures/report.ts";

test("Actions Bun entry writes fork summaries and preserves failure exits without credentials", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-bun-publication-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  for (const ratio of [0.8, 1.2]) {
    const temporary = join(directory, String(ratio));
    await writePaired(join(temporary, "paired"), ratio);
    await cp(join(temporary, "paired/prepared"), join(temporary, "prepared"), { recursive: true });
    const summary = join(temporary, "summary.md");
    const result = spawnSync(
      process.execPath,
      [fileURLToPath(new URL("./publish-report.ts", import.meta.url))],
      {
        encoding: "utf8",
        env: {
          ...process.env,
          PUBLISH: "false",
          GITHUB_TOKEN: "",
          GH_TOKEN: "",
          GITHUB_REPOSITORY: "owner/repo",
          GITHUB_RUN_ID: "42",
          GITHUB_SERVER_URL: "https://github.com",
          // Any accidental network request fails instead of reaching GitHub.
          GITHUB_API_URL: "http://127.0.0.1:1",
          RUNNER_TEMP: temporary,
          GITHUB_STEP_SUMMARY: summary,
          HASH: hash,
          BASE_HASH: baseHash,
          PR_NUMBER: "7",
          BASE: "main",
        },
        timeout: 10000,
      },
    );
    assert.equal(result.status, ratio < 1 ? 0 : 1, result.stderr);
    assert.match(await readFile(summary, "utf8"), new RegExp(`${ratio.toFixed(4)}×`));
    if (ratio > 1) assert.match(result.stderr, /regression alerts/);
    assert.doesNotMatch(result.stderr, /ECONNREFUSED|fetch failed/);
  }
});
