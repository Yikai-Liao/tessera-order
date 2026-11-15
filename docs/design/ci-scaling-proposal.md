# CI and benchmark scaling proposal

Status: steps 1 to 3 implemented; step 4 proposed. Current behavior belongs to [CI](../development/ci.md) and [continuous benchmarks](../development/continuous-benchmarks.md); this page owns the planned changes and their reasons until they land.

## Problems

1. **Every push reruns the whole PR.** The [router](../../.github/scripts/ci_changes.ts) diffs the head against the merge base. Once a PR has touched library source, each later push reruns Rust validation, Miri, AddressSanitizer and all measurement shards, even when that push only edits documentation. One such run of a cached-key PR took about 6.5 minutes on the critical path (preparation 198 s, the slowest shard 176 s) and about 45 runner-minutes in total, against the 4-minute target.
2. **Measurement is all or nothing.** Any library source change measures the complete sorting plan. The serial operations have no continuous cases, so a change confined to an operation entry measures sorting cases it cannot affect and none it can.
3. **Caches thrash.** Repository caches totalled 11.7 GB, above GitHub's 10 GB limit, so least-recently-used eviction removes caches that other jobs expect. The native comparison cache alone is 0.4–0.7 GB per ref, and every PR ref saves its own copies. Long-lived branches that predate a workflow fix keep the old cache keys: PRs into the algorithm experiment branch prepared cold until #213 and #215 were applied there.

## Reuse verified results by input fingerprint

Implemented as [lane reuse](../development/ci.md#reuse-of-verified-lanes). The implementation differs from this plan in four ways. Markers are artifacts rather than cache entries, because only an artifact names the run that wrote it. Only runs on the PR's own commits count. A PR that changes the files deciding reuse reuses nothing. A reused measurement copies its check to the new head, so `tools status` needs no change. The reasoning below is kept for later steps.

A lane runs when its inputs differ from inputs that already passed, not when the PR's cumulative diff touches them.

- **Inputs come from the router.** A lane's inputs are the tracked files whose first `ROUTES` row selects that lane, plus files no row names, which select every lane today. The fingerprint hashes the sorted `(path, blob ID)` list from `git ls-tree` at the head. The routing table stays the only owner of which file affects which lane.
- **Success leaves a marker.** A lane that passes saves a small Actions cache entry keyed by lane and fingerprint. Before scheduling, `changes` looks the markers up without downloading anything. A hit skips the lane and links the run that produced the marker in the summary; `ci-ok` treats a reused lane as passed.
- **Why not diff against the previous push.** The previous head may have been cancelled or failed, force pushes and reverts break commit ancestry, and a diff cannot tell whether the earlier run passed. A fingerprint skips only work that verifiably passed for identical inputs. Cache scoping also lets a PR reuse a marker from its base branch or `main` when its lane inputs equal theirs.
- **Measurement compares two revisions.** Its fingerprint also covers the baseline library tree, because retargeting or merging a new base changes the result without changing the head's files.

A docs-only commit inside a verified PR then runs formatting, lints and Docs only. The remaining risk is a lane input that `ROUTES` does not name; that risk exists today, and unrouted paths still fingerprint into every lane.

## Measure operations by entry ownership

Implemented as [operation cases](../development/continuous-benchmarks.md#cases) and [entry-owned selection](../development/ci.md#selecting-benchmark-workloads). Small representative groups guard operation entry and finishing work; sorting cases cover the shared engine. Entry scopes extend the existing path router instead of introducing a second ownership table. Shared and unknown dependencies select complete measurement. The frozen selection determines the hosted matrix, avoiding a fixed runner count for tiny groups.

Reuse remains per lane. Per-group reuse would need independent dependency fingerprints and report composition across runs; it is deferred because selecting entry groups already avoids unrelated measurements without adding historical state. Broader operation coverage and cross-algorithm references belong to step 4.

## Keep cross-algorithm comparisons out of PR timing

Comparisons with other algorithms answer "how does Tessera compare", not "did this change regress". They stay manual: per-operation plans with each operation's reference set, run with `compare algorithms` locally or through the diagnostics workflow, and a release plan that covers every released operation once per release. PR CI keeps only the comparison lane's correctness checks: adapters build and their outputs verify.

## Cache hygiene

- Implemented: Cargo caches are saved from `main` only ([CI](../development/ci.md#lanes)).
- Keep native build products out of the native comparison cache unless their reuse saves more time than their download costs.
- Long-lived branches take workflow fixes from `main` promptly, without waiting for an engine merge.

## Order

1. Cache hygiene: workflow-only and immediately measurable on preparation time.
2. Fingerprint reuse for the existing lanes.
3. Operation cases in the continuous plan (implemented; entry-ownership selection implemented).
4. Per-operation cross-algorithm plans and their release coverage.

Each step records its measured critical path and runner-minutes in its PR. The [roadmap](../../ROADMAP.md) tracks the steps.
