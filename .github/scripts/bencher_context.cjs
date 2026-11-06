// SPDX-License-Identifier: BSD-2-Clause

// Resolve identity from GitHub, never from files supplied by the PR build.
module.exports = async ({ github, context, core }) => {
  const run = context.payload.workflow_run;
  const repo = context.repo;
  const fullName = `${repo.owner}/${repo.repo}`;
  const { data: workflow } = await github.rest.actions.getWorkflow({
    ...repo, workflow_id: run.workflow_id,
  });
  if (workflow.path !== ".github/workflows/bench-build.yml") {
    throw new Error("Unexpected source workflow");
  }

  if (run.event === "push" || run.event === "workflow_dispatch") {
    if (run.head_branch !== "main" || run.head_repository.full_name !== fullName) {
      throw new Error("Only main can update the baseline");
    }
    core.setOutput("branch", "main");
    core.setOutput("hash", run.head_sha);
    core.setOutput("eligible", "true");
    return;
  }
  if (run.event !== "pull_request") {
    throw new Error("Unexpected source event");
  }

  // workflow_run.pull_requests can be empty for fork PRs. The API also
  // prevents an artifact from selecting another PR or poisoning main.
  const pulls = await github.paginate(
    github.rest.repos.listPullRequestsAssociatedWithCommit,
    { ...repo, commit_sha: run.head_sha, per_page: 100 },
  );
  const matching = pulls.filter(pr =>
    pr.state === "open" && pr.head.sha === run.head_sha &&
    pr.base.repo.full_name === fullName
  );
  if (matching.length === 0) {
    core.notice("PR closed or superseded; no benchmark will be submitted.");
    return;
  }
  if (matching.length !== 1) {
    throw new Error("Ambiguous PR for benchmark commit");
  }
  const pr = matching[0];
  core.setOutput("branch", `pr-${pr.number}`);
  core.setOutput("hash", pr.head.sha);
  core.setOutput("base", pr.base.ref);
  core.setOutput("base_hash", pr.base.sha);
  core.setOutput("number", String(pr.number));
  core.setOutput("eligible", "true");
};
