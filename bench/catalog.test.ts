// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  algorithm,
  caseLabel,
  caseParts,
  catalog,
  comparisonId,
  criterionGroup,
  criterionId,
  distribution,
  modeName,
  parseComparisonId,
  parseCriterionId,
  rawCaseParts,
  recordType,
} from "./catalog.ts";
import { decodeConfig } from "./config.ts";

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
    assert.ok(["common", "rust", "native", "tessera-par", "rayon-stable"].includes(entry.adapter));
    assert.equal(typeof entry.stable, "boolean");
    assert.equal(Boolean(entry.threads), ["tessera-par", "rayon-stable"].includes(entry.adapter));
  }
});

test("both benchmark grammars match the shared language fixture", () => {
  const fixture = decodeConfig(
    readFileSync(new URL("./plans/selection.fixture.toml", import.meta.url), "utf8"),
    "selection.fixture.toml",
  ) as {
    identities: {
      type: string;
      distribution: string;
      length: number;
      mode: string;
      comparison: string;
      criterion_group: string;
      criterion: string;
    }[];
  };
  for (const entry of fixture.identities) {
    const input = { ty: entry.type, distribution: entry.distribution, len: entry.length };
    assert.equal(comparisonId(input), entry.comparison);
    assert.equal(criterionId(input, entry.mode), entry.criterion);
    assert.equal(criterionGroup(entry.criterion), entry.criterion_group);
    assert.deepEqual(parseComparisonId(entry.comparison), { ...input, seed: "s1" });
    assert.deepEqual(parseCriterionId(entry.criterion), {
      ...input,
      seed: "s1",
      mode: entry.mode,
    });
  }
});

test("strict identities reject unknown fields without tightening historical labels", () => {
  const id = "rust/u64/random/65536/s1/api";
  for (const invalid of [
    id.replace("rust/", "other/"),
    id.replace("u64", "future-type"),
    id.replace("random", "future-shape"),
    id.replace("65536", "0"),
    id.replace("s1", "seed"),
    id.replace("api", "future-mode"),
    `${id}/extra`,
  ])
    assert.throws(() => parseCriterionId(invalid));
  assert.throws(() => parseComparisonId(id));
  assert.throws(() => parseComparisonId("u64/random/not-a-length/s1"));
  assert.deepEqual(rawCaseParts(id), {
    input: "u64 · random",
    sub: "65536 · s1 · api",
  });
  assert.deepEqual(rawCaseParts("old/id"), { input: "id", sub: "" });
  assert.equal(caseParts("not-an-id").input, "not-an-id");
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
