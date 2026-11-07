// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { publishImages, type ChartImage } from "./images.ts";
import { mockGitHub } from "./fixtures/github.ts";

function publisher(existing: boolean) {
  const mock = mockGitHub(request => {
    if (request.path.includes("matching-refs")) return existing
      ? [{ ref: "refs/heads/benchmark-images", object: { sha: "old-commit" } }]
      : [{ ref: "refs/heads/benchmark-images-other", object: { sha: "unrelated" } }];
    if (request.path.endsWith("/commits/old-commit")) return { tree: { sha: "old-tree" } };
    if (request.path.endsWith("/trees")) return { sha: "new-tree" };
    if (request.path.endsWith("/commits")) return { sha: "new-commit" };
    return {};
  });
  return {
    ...mock,
    context: { repo: { owner: "o", repo: "r" }, payload: {}, serverUrl: "https://github.com", runId: 1 },
    language: "rust", variant: "default", hash: "a".repeat(40), number: "8",
    images: [{ kind: "comparison", svg: "<svg>comparison</svg>" }, { kind: "trend", svg: "<svg>trend</svg>" }] satisfies ChartImage[],
  };
}

test("initializes an image-only branch and returns immutable raw image URLs", async () => {
  const action = publisher(false);
  const links = await publishImages(action);
  assert.deepEqual(links, {
    comparison: "https://raw.githubusercontent.com/o/r/new-commit/pr-8/rust/default/comparison.svg",
    trend: "https://raw.githubusercontent.com/o/r/new-commit/pr-8/rust/default/trend.svg",
  });
  const tree = action.requests.find(r => r.path.endsWith("/trees"))!.body;
  assert.equal(tree.base_tree, undefined);
  assert.deepEqual((tree.tree as { path: string }[]).map(file => file.path), ["pr-8/rust/default/comparison.svg", "pr-8/rust/default/trend.svg"]);
  assert.deepEqual(action.requests.find(r => r.path.endsWith("/commits"))!.body.parents, []);
  assert.deepEqual(action.requests.at(-1)!.body, { ref: "refs/heads/benchmark-images", sha: "new-commit" });
});

test("preserves other charts through the existing tree and never force-updates the image branch", async () => {
  const action = publisher(true);
  await publishImages(action);
  assert.equal(action.requests.find(r => r.path.endsWith("/trees"))!.body.base_tree, "old-tree");
  assert.deepEqual(action.requests.find(r => r.path.endsWith("/commits"))!.body.parents, ["old-commit"]);
  assert.deepEqual(action.requests.at(-1)!.body, { sha: "new-commit", force: false });
  assert.match(action.requests.at(-1)!.path, /benchmark-images$/);
});

test("does not write an empty image set", async () => {
  const action = publisher(false);
  assert.deepEqual(await publishImages({ ...action, images: [] }), {});
  assert.equal(action.requests.length, 0);
});
