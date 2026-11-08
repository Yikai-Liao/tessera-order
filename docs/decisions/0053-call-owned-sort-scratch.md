<!-- SPDX-License-Identifier: BSD-2-Clause -->

# 0053: Keep sorting scratch owned by each call

Date: 2026-11-08. Status: accepted.

## Context

The unreleased Rust crate exposed both `stable_sort` and `stable_sort_by_key` as free functions and as methods on a reusable `Workspace<T>`. The workspace added a second scratch-ownership protocol, retained-capacity accounting, a release method and separate benchmark cases. The crate has not been published, so removing this surface now does not require a deprecation period.

## Decision

Expose sorting only through `stable_sort` and `stable_sort_by_key`. Delete `Workspace<T>` and its methods immediately, without a compatibility alias or deprecation layer. Each call owns its scratch and releases it on return or unwind. Private buffers may reuse capacity across nodes and passes within that call; callers cannot retain them.

Keep ordered and constant inputs allocation-free. Preserve ordering, stability, key-extraction and panic guarantees, owned and borrowed record handling, `no_std` block-panic abort behavior and the documented memory bounds. Routing thresholds, digit planning and inlining policy do not change.

Remove the benchmark's `workspace` mode and its catalog and plan selections. Preserve the continuous suite's 50 input cases, now measured through 50 `api` IDs, and leave comparison harness API modes unchanged. Git source/build workspaces in benchmark tooling are unrelated and remain.

## Consequences

Existing pre-release callers must replace workspace method calls with the corresponding free functions and remove retained-memory management. There is one public sorting protocol and no caller-controlled scratch lifetime. Tests cover scratch release after a call and allocation-free ordered inputs; panic and conformance tests use the free functions.

Current guides, navigation and plans describe only the remaining API. Earlier decision records retain their historical workspace references. No timed measurements accompany this simplification, and it makes no performance claim.
