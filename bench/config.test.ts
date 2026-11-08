// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { decodeConfig } from "./config.ts";

test("formats decode the same structure", () => {
  const expected = { schema: 1, include: [{ types: ["u64"], lengths: [1000] }] };
  assert.deepEqual(decodeConfig(JSON.stringify(expected), "plan.json"), expected);
  assert.deepEqual(
    structuredClone(
      decodeConfig(
        "# Input selection\nschema = 1\n[[include]]\ntypes = ['u64']\nlengths = [1_000]\n",
        "plan.toml",
      ),
    ),
    expected,
  );
});

test("syntax failures identify the file", () => {
  for (const [filename, text] of [
    ["broken.json", "{"],
    ["broken.toml", "types = ["],
  ]) {
    assert.throws(
      () => decodeConfig(text, filename),
      (error: unknown) =>
        error instanceof Error && error.message.includes(filename) && error.cause instanceof Error,
    );
  }
  assert.throws(() => decodeConfig("schema: 1", "plan.yaml"), /expected .json or .toml/);
});
