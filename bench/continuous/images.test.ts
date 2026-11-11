// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { mockGitHub } from "./fixtures/github.ts";
import { publishImages } from "./images.ts";

for (const status of [422, 403]) {
  test(`image publication ${status === 422 ? "retries a competing ref write" : "retains permission failures"}`, async () => {
    let refs = 0;
    let commits = 0;
    let updates = 0;
    const mock = mockGitHub((request) => {
      if (request.path.includes("/matching-refs/"))
        return [
          { ref: "refs/heads/benchmark-images", object: { sha: ++refs === 1 ? "old" : "new" } },
        ];
      if (request.method === "GET" && request.path.includes("/git/commits/"))
        return { tree: { sha: request.path.endsWith("/old") ? "tree-old" : "tree-new" } };
      if (request.path.endsWith("/trees")) return { sha: "image-tree" };
      if (request.path.endsWith("/commits")) return { sha: `images-${++commits}` };
      if (request.method === "PATCH" && ++updates === 1)
        return new Response(JSON.stringify({ message: "Ref write failed" }), {
          status,
          headers: { "content-type": "application/json" },
        });
      return {};
    });
    const publish = () =>
      publishImages({
        github: mock.github,
        context: {
          repo: { owner: "o", repo: "r" },
          serverUrl: "https://github.com",
          runId: 7,
          payload: {},
        },
        language: "rust",
        hash: "a".repeat(40),
        number: "7",
        images: [{ kind: "comparison", svg: "<svg/>" }],
      });
    if (status === 403) {
      await assert.rejects(publish());
      assert.equal(refs, 1);
    } else {
      assert.match((await publish()).comparison!, /\/images-2\/pr-7\/rust\/comparison.svg$/);
      assert.deepEqual(
        mock.requests
          .filter((request) => request.path.endsWith("/trees"))
          .map((request) => request.body.base_tree),
        ["tree-old", "tree-new"],
      );
    }
  });
}
