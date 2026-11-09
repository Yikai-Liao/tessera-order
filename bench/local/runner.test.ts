// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import { parse, stringify } from "smol-toml";
import { serialBaselineManifest } from "./runner.ts";

test("pre-parallel serial baselines retain defaults and the shared locked dependency graph", async () => {
  const current = await readFile(
    new URL("../../rust/tessera-sort/Cargo.toml", import.meta.url),
    "utf8",
  );
  const historical = parse(current);
  delete (historical.features as Record<string, unknown>).rayon;
  delete (historical.dependencies as Record<string, unknown>).rayon;
  const baseline = stringify(historical);
  const completed = parse(serialBaselineManifest(baseline, current));
  assert.deepEqual(completed, parse(current));
  assert.deepEqual(
    (completed.features as Record<string, unknown>).default,
    (historical.features as Record<string, unknown>).default,
  );
  assert.equal(serialBaselineManifest(current, current), current);
  const conflicting = parse(baseline);
  (conflicting.dependencies as Record<string, unknown>).rayon = "1";
  assert.throws(() => serialBaselineManifest(stringify(conflicting), current), /explicit review/);
  const mandatory = parse(current);
  (mandatory.dependencies as Record<string, { optional: boolean }>).rayon.optional = false;
  assert.throws(() => serialBaselineManifest(baseline, stringify(mandatory)), /optional Rayon/);
});
