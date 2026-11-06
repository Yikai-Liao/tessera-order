# Versions and releases

Each language package is versioned and released on its own ([decision 0003](docs/decisions/0003-independent-package-versions.md)). No package has been released yet; the versions below are examples.

| Object | Identifier | Identifies |
|---|---|---|
| Rust crate | SemVer, tag `rust/v0.3.1` | Rust API, implementation and fixes |
| C++ library (future) | SemVer, tag `cpp/v0.1.2` | C++ API, implementation and fixes |
| Python package (planned) | SemVer, tag `python/v0.1.0` | Python API, adapters and the Rust core it embeds |
| Repository | Full Git commit | All implementations, tests, docs and benchmarks together |

- Stable and unstable interfaces within one language share its package version.
- Bump only the packages whose shipped contents change. Changes to documentation or benchmarks alone do not release a library.
- There is no shared algorithm version. Rust and C++ may run the same algorithm at different versions, or differ in optimizations for a while.
- An issue that affects several languages tracks each language's affected, fixed and validated status separately. Do not call it fixed in a language that has not been validated.
- Future Go modules are tagged by module directory, for example `go/tessera/v0.1.0` for `go/tessera/`.

## Compatibility

| Change | From 1.0 | During 0.x |
|---|---|---|
| Bug fix, or internal change that keeps every public contract | Patch | Patch |
| New public API or capability | Minor | Minor |
| Removed API or incompatible behavior | Major | Minor, with migration notes |

Cargo treats `0.3` to `0.4` as incompatible. The size of a performance change never decides the increment.

Public contracts are stability; float, NaN and signed-zero order; supported key and record types; record preservation after a panic or exception; and documented auxiliary-space bounds. Replacing the kernel behind an unstable interface is compatible only if that interface already left equal-key order unspecified and every other guarantee holds. The changelog names the kernel change.

## Release checklist

1. Run the language's correctness, recovery, platform and package checks, and the performance checks in [`docs/development/benchmarking.md`](docs/development/benchmarking.md).
2. Update `CHANGELOG.md` and the README capability table.
3. Before the first publication, tell Robert Clausecker that the project builds on his block partition.
4. Push the tag. Never move a published tag or replace a published version; fix a bad release with the next version.

Benchmark manifests record the implementation (for example `tessera-rust`), package version, full commit, actual kernel, toolchain and build flags. Cross-language results state whether they compare one commit or each language's latest release. Uncommitted code also needs source hashes or a patch, because a commit alone does not identify it.

## CI and release workflows

`ci.yml` selects languages from the changed paths and calls their workflows in parallel:

| Changed paths | Runs |
|---|---|
| `rust/`, `bench/rust/`, root Cargo and rustfmt files | Rust, and Python because it embeds the Rust core |
| `cpp/`, `bench/cpp/`, root CMake files | C++ |
| `python/`, `bench/python/` | Python |
| Anything else (semantics, vectors, docs, license, workflows) | Every implemented language |

A manual run selects everything. In `rust-ci.yml`, tests, lints, packaging, Miri and ASan are separate jobs.

`release.yml` routes each tag to its language's release workflow, which runs that language's CI, checks the tag against the package metadata and publishes. Each tag has its own concurrency group, so languages can release in parallel.

| Language | Active when | Release |
|---|---|---|
| Rust | Now. Publishing needs `publish = false` removed and the `CARGO_REGISTRY_TOKEN` secret | crates.io and a GitHub release with the `.crate` file |
| C++ | `cpp/CMakeLists.txt` exists. It must honor `BUILD_TESTING`, register tests with CTest and configure CPack with the package version | CI configures, builds and runs CTest on Linux x86-64 and AArch64, macOS and Windows; the release checks `CPACK_PACKAGE_VERSION` against the tag and attaches a TGZ package to a GitHub release |
| Python | `python/pyproject.toml` and `python/Cargo.toml` exist | GIL-enabled wheels for CPython 3.11 and 3.13 on the same four platforms, uploaded to PyPI |

Until then, CI skips the language and says so in the job summary; there is no placeholder job that passes without testing anything.

The PyPI upload runs in `release.yml` itself because [PyPI does not accept a reusable workflow as a trusted publisher](https://docs.pypi.org/trusted-publishers/troubleshooting/#reusable-workflows-on-github). Before the first Python release: configure the trusted publisher, confirm the supported interpreter and ABI matrix, record the core source and toolchain in each wheel build, and add install tests for the release wheels. See the [Python binding proposal](docs/proposals/python-binding.md).
