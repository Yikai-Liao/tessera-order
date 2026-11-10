// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

/** Protect the feedback/measurement boundary without imposing unrelated YAML layout. */
function validate(workflow: string) {
  const queue = workflow.slice(workflow.indexOf("  queue:"), workflow.indexOf("  rust:"));
  const rust = workflow.slice(workflow.indexOf("  rust:"), workflow.indexOf("  finalize:"));
  const finalize = workflow.slice(workflow.indexOf("  finalize:"));
  assert.match(queue, /queueReport/);
  assert.match(queue, /publishProgress/);
  assert.doesNotMatch(queue, /concurrency:|environment: benchmark|BENCHER_API_KEY/);
  assert.match(rust, /needs: queue/);
  assert.match(rust, /group: bencher-bare-metal\n\s+queue: max/);
  assert.match(rust, /Revalidate source after waiting[\s\S]*resolveSource/);
  assert.match(rust, /activateReport/);
  assert.doesNotMatch(rust, /queueReport/);
  assert.match(rust, /needs.queue.outputs.check_default/);
  assert.match(rust, /needs.queue.outputs.check_simd/);
  assert.match(finalize, /needs: \[queue, rust\]/);
  assert.match(finalize, /if: always\(\)/);
  assert.match(finalize, /needs.queue.result == 'cancelled'/);
  assert.match(finalize, /finishPendingReports/);
  assert.doesNotMatch(finalize, /concurrency:|environment: benchmark|BENCHER_API_KEY/);
  for (const job of [queue, rust, finalize]) {
    assert.match(job, /contents: (?:read|write)/);
    assert.match(job, /ref: \$\{\{ github.workflow_sha \}\}/);
    assert.match(job, /persist-credentials: false/);
    assert.match(job, /--frozen-lockfile --ignore-scripts/);
    assert.match(job, /pnpm\/action-setup@/);
    assert.match(job, /node-version: 24/);
    const checkout = job.indexOf("github.workflow_sha");
    const install = job.indexOf("run: pnpm install --frozen-lockfile --ignore-scripts");
    const lifecycle = job.indexOf("require('./bench/continuous/lifecycle.ts')");
    assert.ok(checkout >= 0 && checkout < install && install < lifecycle);
  }
}

test("tracking publishes before the shared queue and finalizes after interruption", async () => {
  const source = await readFile(
    new URL("../../.github/workflows/bench-track.yml", import.meta.url),
    "utf8",
  );
  validate(source);
  for (const [from, to] of [
    ["queueReport", "activateReport"],
    ["Revalidate source after waiting", "No revalidation"],
    ["needs: [queue, rust]", "needs: rust"],
    ["if: always()", "if: success()"],
    ["needs.queue.result == 'cancelled'", "false"],
    ["github.workflow_sha", "github.event.workflow_run.head_sha"],
    ["group: bencher-bare-metal", "group: other"],
  ]) {
    const mutant = source.replaceAll(from, to);
    assert.notEqual(mutant, source);
    assert.throws(() => validate(mutant), from);
  }
});
