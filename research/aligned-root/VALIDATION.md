# Checkpoint validation

The standalone staged package was validated on 2026-11-07. Build artifacts were
written outside the source tree. No benchmark runner or raw results are included.

Passed:

- `cargo test --all-targets`: 6 unit tests and 1 integration test.
- `cargo test --release --all-targets`: the same 7 tests.
- `cargo test --doc`: no doctest failures (0 examples).
- `cargo +1.85 test --all-targets`: the same 7 tests on the declared MSRV.
- `cargo clippy --all-targets -- -D warnings`.
- `cargo +nightly miri test --lib actual_parallel_tiny_boundary_and_restore`:
  default Stacked Borrows; actual concurrent scatter/restoration closures via
  standard scoped threads, 2.20 seconds.
- `cargo +nightly miri test --lib packed_short_split_cycles_and_uninitialized_spares`:
  default Stacked Borrows; forced short and split cycles, with uninitialized
  spare scratch, 0.97 seconds.

The Miri executor substitution and its scope are documented in README.md.
Rayon scheduler execution itself is not claimed Miri-clean. Native tests use
Rayon. The vendored dependency source files match the measured frozen dependency
byte-for-byte, and the library files match the final measured Direct-child
control byte-for-byte. The dedicated direct-u64 integration test is additional.

Performance interpretation:

- v1 and v2 u64 pilots used by-key APIs for both the combined and serial controls.
  They must not be presented as Direct-u64 baseline results.
- The final v3 control separately compares Direct combined/serial baselines and
  cached versus Direct child kernels in one process. Its controls were noisy.
- No end-to-end performance win is established. See README.md for the separate
  figures, control ranges, and exact counting-allocator peaks.
