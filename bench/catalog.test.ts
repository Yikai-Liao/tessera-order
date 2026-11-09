// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { algorithm, caseLabel, catalog, distribution, modeName, recordType } from "./catalog.ts";

test("catalog identities are unique and references resolve", () => {
  assert.equal(catalog.schema, 1);
  for (const entries of [
    catalog.algorithms,
    catalog.types,
    catalog.distributions,
    catalog.modes,
    catalog.configurations,
  ]) {
    assert.equal(new Set(entries.map((entry) => entry.id)).size, entries.length);
    for (const entry of entries) assert.match(entry.id, /^[a-z0-9][a-z0-9._-]*$/);
  }
  for (const id of catalog.comparison.types) assert.notEqual(recordType(id).task, "unknown");
  for (const id of catalog.comparison.shapes) assert.ok(distribution(id).group);
  for (const id of catalog.comparison.summaryShapes)
    assert.ok(catalog.comparison.shapes.includes(id));
  for (const group of catalog.comparison.typeGroups)
    for (const id of group.types) assert.ok(catalog.comparison.types.includes(id));
  for (const entry of catalog.algorithms) {
    assert.ok(["radix", "comparison", "SIMD"].includes(entry.family));
    assert.ok(["common", "rust", "native", "revision"].includes(entry.adapter));
    assert.equal(typeof entry.stable, "boolean");
  }
});

test("unknown historical identities retain readable IDs", () => {
  assert.equal(algorithm("future-sort").label, "future-sort");
  assert.equal(distribution("future-shape").label, "future-shape");
  assert.equal(modeName("future-mode"), "future-mode");
  assert.equal(
    caseLabel("rust/future-type/future-shape/1000/s2/future-mode"),
    "future-type · future-shape · 1,000, seed 2, future-mode",
  );
});
