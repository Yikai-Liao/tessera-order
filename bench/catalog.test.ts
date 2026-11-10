// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  caseParts,
  comparisonId,
  criterionGroup,
  criterionId,
  parseComparisonId,
  parseCriterionId,
  rawCaseParts,
} from "./catalog.ts";
import { decodeConfig } from "./config.ts";

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
