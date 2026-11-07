# 0004: Run continuous benchmarks on Bencher Bare Metal

Date: 2026-11-06. Status: accepted. The alert rule in the last decision bullet is superseded by [0029](0029-compare-with-the-previous-main-sample.md).

## Context

PRs need performance feedback before the release runner exists. Shared GitHub Actions hosts are too noisy for timing. Bencher Cloud's Free plan runs OCI images on bare metal for public projects, one job per organization, at most 5 minutes each. Fork PRs must not reach secrets or run on a privileged host. The repository will hold Rust, C++ and Python packages.

## Decision

- Each language uses its standard benchmark harness and the matching Bencher adapter: Criterion for Rust, later Google Benchmark for C++ and pytest-benchmark for Python. Benchmarks are named `<language>/<case_id>/<alloc_mode>` with the protocol's case IDs, so the languages stay comparable without a shared runner.
- An unprivileged workflow builds a static executable per language. A trusted `workflow_run` workflow resolves the PR through GitHub, wraps the executable in an image it defines itself and submits the image digest.
- Each language is its own Bencher job under 240 seconds. All jobs share one GitHub concurrency group that queues them.
- `main` is the baseline; alerts use a t-test against its recent history and do not fail the workflow.

## Consequences

- Compilation does not count toward the 5-minute job limit, and fork PRs use the same pipeline.
- The repository maintains no timing code for continuous benchmarks. The release runner is still needed for A/A controls, the full matrix and memory accounting.
- Languages measure with different harnesses, so cross-language comparisons of these numbers are indicative only.
- Synchronous waiting spends Actions minutes but needs no callback service. The concurrency group does not coordinate other repositories in the same Bencher organization.

Details are in [Continuous benchmarks](../development/continuous-benchmarks.md).
