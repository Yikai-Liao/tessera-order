# 0004: Support no-std with alloc behind a default-on std feature

Date: 2026-11-06. Status: accepted. The panic-strategy requirement and its safety
rationale are superseded by [0005](0005-no-std-unwind-guard.md).

## Context

The roadmap deferred `no_std` until after the first releases, and the README
said the crate needs `std`. The engine, however, uses very little of `std`: of
62 qualified `std` paths, 57 are `core` items reached through the wrong crate
root. Only two things are really `std`:

- `std::panic::catch_unwind` and `resume_unwind`, which let a key-function panic
  be caught mid-pass, completed into a valid pass, and resumed after every
  record has been moved back into the caller's slice;
- `f64::sqrt` and `f64::ceil` in `default_block_size`, because `core` has no
  floating-point intrinsics.

Everything else — `Vec`, `Box`, `vec!`, `slice::sort_unstable` and
`AssertUnwindSafe` — is available from `alloc` and `core`.

## Decision

Ship `no_std` support as an optional `alloc`-backed build:

- A `std` feature, enabled by default, so existing users change nothing and
  `default-features = false` selects the `no_std` build.
- The crate declares `#![cfg_attr(not(feature = "std"), no_std)]` and takes
  `Vec` and `Box` from `alloc`. A `no_std` build therefore needs a global
  allocator; it does not need a `core`-only code path.
- Panic recovery moves into one internal `unwind` module. With `std` it keeps
  `catch_unwind`. Without `std` there is no unwinder to catch with, so panics
  must abort and the crate's record-restoration guarantee is not observable:
  the process ends before a caller could inspect the slice.
- `default_block_size` keeps the prototype's floating-point formula under `std`
  and solves `m * m * R * b >= 72 n` in integers otherwise. Both forms return
  the same number; `scaled_forms_agree` in `block.rs` checks them against each
  other over lengths, record sizes and radix widths, so the measurements behind
  the constant still describe the sort.
- No dependency is added. `libm` would have replaced the integer square root,
  but the crate is zero-dependency by policy, and the integer form is exact.

## Alternatives considered

- **`libm` for `sqrt` and `ceil`.** Rejected: adds a dependency to save about
  twenty lines, and the floating-point form is only needed for the block size.
- **Removing `catch_unwind` altogether** and repairing a half-scattered pass from
  `Drop`. This would give `std` and `no_std` identical semantics, but it needs
  the scatter state restructured into the `Sorter` and a repair path that runs
  without allocating while unwinding. Rejected for now: it trades a well-tested
  recovery path for a harder one, and it is not a prerequisite for `no_std`.
- **A `core`-only build with caller-provided scratch.** Worth doing only when a
  user without a global allocator appears; it replaces the buffer layer rather
  than the panic or floating-point detail. Left in the roadmap.

## Consequences

- `default-features = false` requires `panic = "abort"` in the final binary. A
  build that unwinds without `std` would leak records when a key function
  panics: safe, but it breaks the contract that the slice holds each record
  exactly once. The documentation states the requirement and does not try to
  detect the panic strategy, which a library cannot observe reliably.
- The panic-recovery tests, in-crate and in `tests/recovery.rs`, run only with
  the `std` feature.
- CI builds the crate for `thumbv7em-none-eabi` without default features and
  runs clippy and the tests with the feature off on the host target.
- docs.rs builds with all features, so the published documentation describes the
  `std` build.
