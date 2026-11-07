# 0016: Write commit and pull request titles as Conventional Commits

Date: 2026-11-07. Status: accepted.

## Context

Commit summaries on `main` were plain imperative phrases. They said what changed but not what kind of change it was, so finding every performance change or every fix meant reading the whole log. Pull requests are squash-merged, so the pull request title, not the branch commits, becomes the commit on `main`.

## Decision

Commit messages and pull request titles follow [Conventional Commits 1.0](https://www.conventionalcommits.org/en/v1.0.0/) with the types, scopes and length limit listed in [`CONTRIBUTING.md`](../../CONTRIBUTING.md#commit-messages). The `PR title` workflow checks the title with `.github/scripts/check_pr_title.py` on every edit and push; branch commits are not checked, because the squash merge replaces them. Dependabot titles use the `build(deps)` and `ci(deps)` prefixes.

## Consequences

- `git log --grep '^perf'` and similar queries select history by kind, and each `perf` commit points at measured work.
- The type and scope lists live in the check script and `CONTRIBUTING.md`; a new type or scope changes both, and language packages add their scope when they land.
- `CHANGELOG.md` stays hand-written. The types classify commits but do not decide what users need to read.
- Commits on `main` before this decision keep their plain summaries.
