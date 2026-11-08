// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { marker } from "./continuous/github.ts";
import { selectFeedback } from "./pr.ts";

test("feedback excludes stale and untrusted comments", () => {
  const pull = { number: 42, head: { sha: "current" }, html_url: "pr-url" };
  const comment = (hash: string, login: string) => ({
    body: marker("rust", hash),
    html_url: hash,
    user: { login },
  });
  const comments = [
    comment("old", "github-actions[bot]"),
    comment("current", "contributor"),
    comment("current", "github-actions[bot]"),
  ];
  const result = selectFeedback(pull, [], comments);
  assert.equal(result.comment?.url, "current");
  assert.equal(selectFeedback(pull, [], comments.slice(0, 2)).comment, null);
});
