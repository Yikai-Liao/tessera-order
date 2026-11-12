// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import ts from "typescript6";
import { nextTick, ref, shallowRef, watch } from "vue";

test("performance history recovers from failed downloads into new and cached ranges", async () => {
  // Execute the component-owned watcher rather than copying its state machine.
  // No DOM or live endpoint is needed to exercise cancellation and cache recovery.
  const component = await readFile(new URL("./PerformanceHistory.vue", import.meta.url), "utf8");
  const start = component.indexOf("  watch(months, async");
  const end = component.indexOf("\n});", start);
  assert.ok(start > 0 && end > start);
  const months = ref<string[]>([]);
  const index = shallowRef<object | null>({});
  const journals = shallowRef(new Map<string, unknown[]>());
  const error = ref("");
  const loading = ref(false);
  const failed = new Set(["2026-10"]);
  const requested: string[] = [];
  const download = async (_source: string, file: string) => {
    requested.push(file);
    if (failed.has(file.slice(0, 7))) throw new Error("temporary outage");
    return "[]";
  };
  const install = new Function(
    "watch",
    "months",
    "index",
    "journals",
    "error",
    "loading",
    "download",
    "props",
    "readable",
    "parseRuns",
    "fail",
    ts.transpileModule(`return ${component.slice(start, end).trim()}`, {
      compilerOptions: { target: ts.ScriptTarget.ES2023 },
    }).outputText,
  );
  const stop = install(
    watch,
    months,
    index,
    journals,
    error,
    loading,
    download,
    { source: "/local/" },
    (read: () => unknown) => read(),
    JSON.parse,
    (cause: Error) => {
      error.value = cause.message;
    },
  );
  const settle = async () => {
    await nextTick();
    await new Promise<void>((resolve) => setImmediate(resolve));
  };
  try {
    months.value = ["2026-10"];
    await settle();
    assert.equal(error.value, "temporary outage");
    assert.equal(loading.value, false);
    months.value = ["2026-11"];
    await settle();
    assert.equal(error.value, "");
    assert.ok(journals.value.has("2026-11"));
    months.value = ["2026-10"];
    await settle();
    assert.equal(error.value, "temporary outage");
    const count = requested.length;
    months.value = ["2026-11"];
    await settle();
    assert.equal(error.value, "");
    assert.equal(loading.value, false);
    assert.equal(requested.length, count);
    // An index failure is different: no known-valid index means no range recovery.
    index.value = null;
    error.value = "index unavailable";
    months.value = [];
    await settle();
    assert.equal(error.value, "index unavailable");
  } finally {
    stop();
  }
});
