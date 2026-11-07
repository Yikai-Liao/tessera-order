# 0001c: Version and release each language package independently

Date: 2026-11-06. Status: accepted.

## Context

The repository will hold a Rust crate, a C++ library and a Python binding that share one design and one set of ordering rules. The languages will not always reach the same fix or optimization at the same time.

## Decision

Each language package has its own SemVer version and tag prefix (`rust/v`, `cpp/v`, `python/v`) and releases on its own schedule. Stable and future unstable interfaces within one language share that language's version. There is no separate algorithm version; a full Git commit identifies a snapshot of the whole repository.

## Consequences

- A fix can ship in one language while another is still affected; issues track each language's status separately.
- CI runs one workflow per language, selected by changed paths. Each release workflow validates and publishes only its own package.
- Benchmark manifests name the implementation, package version, commit and actual kernel, so results from different packages can be compared.

The rules are in [`RELEASING.md`](../../RELEASING.md).
