# Packaging verification

On 2026-11-07, all six implementation/test/manifest files matched the originating frozen
final SHA-256 manifest byte-for-byte. The benchmark CLI and its allocator helper are
excluded. This standalone crate is not part of the production Cargo workspace.

The packaged location passed offline, single-build-job checks under the shared CPU lock:

- Release all-targets: 2 library and 13 integration tests passed, zero failures.
- Clippy all-targets with warnings denied: passed.
- Doc tests: passed, zero examples.

The original implementation and tests previously passed the detailed debug/release and
focused strict-provenance Stacked Borrows Miri scope recorded in README.md. Packaging did
not repeat Miri or MSRV, and does not claim to have done so. No performance was measured.
No implementation file or manifest was changed by packaging or these checks.

The external packaging log SHA-256 is `22b28bc2e98c435a33317930432db6fd5a634d5aeb4f165d9c362793555b4c2c`.
Raw logs, original benchmark CLI, allocator helper, and binaries are not repository files.
