# 0032: Keep parallel pull requests free of shared bookkeeping

Tooling update: [decision 0050](0050-agent-first-typescript-tooling.md) replaces the historical Python title checker below with `.github/scripts/check_pr_title.ts`.

Date: 2026-11-07. Status: accepted.

## Context

Several pull requests are often open at once, and four rules made them collide. Decision records took the next free number, so two open pull requests claimed the same number and the same row at the end of the index, and the later one had to renumber and fix its links. `main` required a branch to be up to date before merging, so every merge forced the other open pull requests to rebase although their changes did not overlap. Each rebase is a new head, and a Rust change measures again on the one Bencher runner, queued behind the baseline run of every push to `main`. Every user-visible change also added a line to the same section of `CHANGELOG.md`.

Commit summaries on `main` were plain imperative phrases. They said what changed but not what kind of change it was, so finding every performance change or every fix meant reading the whole log. Pull requests are squash-merged, so the pull request title, not the branch commits, becomes the commit on `main`.

## Decision

- **Titles.** Commit messages and pull request titles follow [Conventional Commits 1.0](https://www.conventionalcommits.org/en/v1.0.0/) with the types, scopes and length limit listed in [`CONTRIBUTING.md`](../../CONTRIBUTING.md#commit-messages). The `PR title` workflow checks the title with `.github/scripts/check_pr_title.py` on every edit and push; branch commits are not checked, because the squash merge replaces them. Dependabot titles use the `build(deps)` and `ci(deps)` prefixes.
- **Record numbers.** A decision record takes the number of the pull request that adds it, as Rust RFCs do: open the pull request, then name the file `NNNN-slug.md`. A pull request that adds several records gives them the suffixes `a`, `b`, `c`, as in `0005a` and `0005b`. Numbers have gaps. The earlier records, numbered 0001 to 0015 in order, are renamed after the pull requests that added them, and every reference follows. The index is the file listing and the documentation site's sidebar, which is generated from the files, so no shared table needs a new row.
- **Release notes.** The repository keeps no changelog file. At each release, an agent or a maintainer writes the notes from the pull requests merged since the package's previous tag and the decision records they added, following [`RELEASING.md`](../../RELEASING.md), and publishes them as the GitHub release. Written notes explain a change in users' terms and group related pull requests, which a list generated from titles cannot.
- **Merging.** `main` requires the `ci-ok`, `sign-off` and `pr-title` checks but not an up-to-date branch. A pull request without conflicts merges as it is.

## Consequences

- Two open pull requests never need the same file name or the same line of an index.
- Links to the earlier numbers from outside the repository break once; the records are young and unreleased, so one consistent scheme is worth more than old links.
- A benchmark report stays valid without a rebase: each run is compared with the `main` baseline under [decision 0029](0029-compare-with-the-previous-main-sample.md), and a rebase only adds a run to the queue.
- Two pull requests that pass separately can still break `main` together. CI runs on every push to `main`, and such a break is fixed forward in a new pull request.
- The type and scope lists live in the check script and `CONTRIBUTING.md`; a new type or scope changes both, and language packages add their scope when they land.
- The pull request title and description are the source of the release notes, so they describe a user-visible change in users' terms.
- Commits on `main` before this decision keep their plain summaries; the first release's notes are written from the README and the decision records.
