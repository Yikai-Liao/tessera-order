// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  caseGroup,
  caseId,
  caseLabel,
  caseParts,
  caseSection,
  columnId,
  compareCases,
  criterionId,
  type Execution,
  operationSchema,
  parseCaseId,
  parseColumnId,
  parseCriterionId,
  parseExecution,
  referenceColumn,
  referenceLabel,
  SORT,
} from "./catalog.ts";
import { decodeConfig } from "./config.ts";

interface Fixture {
  identities: {
    case: { type: string; distribution: string; length: number; operation: unknown };
    column: { implementation: string; execution: string };
    case_id: string;
    group: string;
    column_id: string;
    criterion: string;
  }[];
  invalid: { executions: string[]; columns: string[]; case_ids: string[]; criterion: string[] };
  operation_case: { type: string; distribution: string; length: number };
  operations: { operation: unknown; case_id?: string }[];
}

const fixture = decodeConfig(
  readFileSync(new URL("./plans/selection.fixture.toml", import.meta.url), "utf8"),
  "selection.fixture.toml",
) as unknown as Fixture;

test("identities match the shared language fixture in both directions", () => {
  for (const entry of fixture.identities) {
    const input = {
      ty: entry.case.type,
      distribution: entry.case.distribution,
      len: entry.case.length,
      operation: operationSchema.parse(entry.case.operation),
    };
    const column = {
      implementation: entry.column.implementation,
      execution: entry.column.execution as Execution,
    };
    assert.equal(caseId(input), entry.case_id);
    assert.equal(caseGroup(input), entry.group);
    assert.equal(columnId(column), entry.column_id);
    assert.equal(criterionId(input, column.execution), entry.criterion);
    assert.deepEqual(parseCaseId(entry.case_id), input);
    assert.deepEqual(parseColumnId(entry.column_id), column);
    assert.deepEqual(parseCriterionId(entry.criterion), { ...input, execution: column.execution });
  }
});

test("decoders reject noncanonical, unknown or out-of-range components", () => {
  for (const id of fixture.invalid.executions)
    assert.throws(() => parseExecution(id), Error, JSON.stringify(id));
  for (const id of fixture.invalid.columns) assert.throws(() => parseColumnId(id), Error, id);
  for (const id of fixture.invalid.case_ids) assert.throws(() => parseCaseId(id), Error, id);
  for (const id of fixture.invalid.criterion) assert.throws(() => parseCriterionId(id), Error, id);
});

test("operation parameters admit exactly the fixture's operations", () => {
  const { type: ty, distribution, length: len } = fixture.operation_case;
  for (const entry of fixture.operations) {
    const parsed = operationSchema.safeParse(entry.operation);
    if (!entry.case_id) {
      // Rejected by the schema or by the input length.
      assert.ok(
        !parsed.success ||
          (() => {
            try {
              parseCaseId(caseId({ ty, distribution, len, operation: parsed.data }));
              return false;
            } catch {
              return true;
            }
          })(),
        JSON.stringify(entry.operation),
      );
      continue;
    }
    assert.ok(parsed.success, JSON.stringify(entry.operation));
    assert.equal(caseId({ ty, distribution, len, operation: parsed.data }), entry.case_id);
  }
});

test("display fields name sections by operation and execution", () => {
  const sort = "rust/sort/u64/random/65536/s1/serial";
  const parallel = "rust/sort/u64/random/65536/s1/t4";
  const select = "rust/select_nth/u64/random/65536/s1/16384/serial";
  // Operation cases keep the plain input name and get their own figure section.
  assert.equal(caseParts(select).input, caseParts(sort).input);
  assert.equal(caseSection(sort).title, "sort");
  assert.match(caseSection(parallel).title, /4 threads$/);
  assert.match(caseSection(select).title, /16,384/);
  assert.ok(caseSection(sort).order < caseSection(parallel).order);
  assert.ok(caseSection(parallel).order < caseSection(select).order);
  assert.ok(caseLabel(select).includes("65,536"));
  assert.equal(caseParts("not-an-id").input, "not-an-id");
  assert.equal(caseSection("not-an-id").title, "");
  assert.deepEqual([select, "not-an-id", parallel, sort].sort(compareCases), [
    sort,
    parallel,
    select,
    "not-an-id",
  ]);
});

test("each operation names the reference its speeds divide by", () => {
  assert.deepEqual(referenceColumn(SORT, "serial"), {
    implementation: "std-stable",
    execution: "serial",
  });
  assert.deepEqual(referenceColumn(SORT, "t4"), {
    implementation: "rayon-stable",
    execution: "t4",
  });
  assert.equal(referenceLabel({ kind: "select_nth", index: 0 }, "serial"), "slice::sort_unstable");
  assert.equal(referenceColumn({ kind: "argsort" }, "t4"), undefined);
});
