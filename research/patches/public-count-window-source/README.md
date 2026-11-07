# Public Rayon count-window source checkpoint

This is a reviewable source and safety checkpoint, not a default-adoption decision or a performance result. It ports the previously validated narrow Direct count mechanism into the public Rayon entry. It claims no generic, owned-record, full-random or root-pipeline improvement.

Base: public commit `987fbdfe757cc81330db04ee11ed6e454b2e5142`, tree `1a45e496ce7cd0a965b4851f08e002f5f3ed3796`.

Apply `public-count-window.patch` with `git apply` at that repository root. No Cargo manifest, dependency, workspace, lockfile or feature change is part of the patch. Public Cargo files remain exactly as published. The patch contains only six necessary Rust files: three modifications and three additions. Existing native test files are unchanged; new tests are separate.

Production changes are limited to sealed u32/u64 hooks, a default-decline Extract method, Direct forwarding, Narrow forwarding with its bit offset, and the count helper call before the existing narrow-window fallback. The original serial count-fill remains the true fullserial baseline. Pool, Job, bucket provenance, sparse-root handling, root compaction and spawn scheduling are unchanged. engine.rs gains only a test-configuration module declaration.

There is no research TLS, serial-schedule selector, forced-worker knob, new public API, new unsafe, or public ByKey T:Sync requirement. The optional research code from the earlier private kernel was removed by retaining the exact production parallel iterator branches. Constants, plan, scalar count/prefix/fill, allocation checks, and geometry are unchanged.

Contents:
- public-count-window.patch: complete source change.
- SAFETY.md: domain, proof obligations and storage bounds.
- VALIDATION.md: executed safety gates and limitations.
- identities.json: full baseline/candidate Rust file hashes and tested identities.
- replay.json: independent apply/hash/reverse/second-forward results.
- SHA256SUMS: checksums of the other packet files.

No raw benchmark samples, harness, target, executable, full repository copy or local-machine paths are included. Performance results belong in a separate report.
