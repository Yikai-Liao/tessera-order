// SPDX-License-Identifier: BSD-2-Clause

import type { ActionContext, GitHub } from "./github.ts";

export interface ChartLinks {
  comparison?: string;
  trend?: string;
}

export interface ChartImage {
  kind: keyof ChartLinks;
  svg: string;
}

interface ImageOptions {
  github: GitHub;
  context: ActionContext;
  language: string;
  hash: string;
  number?: string;
  images: ChartImage[];
}

// Comments need image URLs, and artifacts are ZIP downloads. A separate
// image-only branch supplies public SVGs; pinning each URL to its commit keeps
// old reports intact. raw.githubusercontent.com serves them as image/svg+xml
// and GitHub embeds that host without a redirect. The tracking workflow
// serializes writers through its concurrency group.
export async function publishImages({ github, context, language, hash, number, images }: ImageOptions): Promise<ChartLinks> {
  if (!images.length) return {};
  const repo = context.repo;
  const branch = "benchmark-images";
  const ref = `refs/heads/${branch}`;
  const { data: matches } = await github.rest.git.listMatchingRefs({ ...repo, ref: `heads/${branch}` });
  const head = matches.find(match => match.ref === ref)?.object.sha;
  const baseTree = head ? (await github.rest.git.getCommit({ ...repo, commit_sha: head })).data.tree.sha : undefined;
  const directory = `${number ? `pr-${number}` : "main"}/${language}`;
  const { data: tree } = await github.rest.git.createTree({
    ...repo, base_tree: baseTree,
    tree: images.map(({ kind, svg }) => ({ path: `${directory}/${kind}.svg`, mode: "100644", type: "blob", content: svg })),
  });
  const { data: commit } = await github.rest.git.createCommit({
    ...repo, tree: tree.sha, parents: head ? [head] : [],
    author: { name: "github-actions[bot]", email: "41898282+github-actions[bot]@users.noreply.github.com" },
    message: `Publish ${language} benchmark charts for ${number ? `PR #${number}` : "main"} (${hash.slice(0, 7)})\n\n` +
      "Signed-off-by: github-actions[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
  });
  if (head) {
    await github.rest.git.updateRef({ ...repo, ref: `heads/${branch}`, sha: commit.sha, force: false });
  } else {
    await github.rest.git.createRef({ ...repo, ref, sha: commit.sha });
  }
  const links: ChartLinks = {};
  for (const { kind } of images) {
    links[kind] = `https://raw.githubusercontent.com/${repo.owner}/${repo.repo}/${commit.sha}/${directory}/${kind}.svg`;
  }
  return links;
}
