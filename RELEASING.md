# Versions and releases

Each independent implementation is versioned and released independently so fixes need not wait for other implementations. Bindings that embed the Rust core synchronize their `X.Y.Z` version with the embedded Rust crate so users can tell which core engine version and performance characteristics a package carries. The versions below are examples.

| Object | Identifier | Identifies |
|---|---|---|
| Rust crate | SemVer, tag `rust/v0.3.1` | Rust API, implementation and fixes |
| C++ library (future) | SemVer, tag `cpp/v0.1.2` | C++ API, implementation and fixes |
| Python package | PEP 440 synced to Rust `X.Y.Z`, tag `python/v0.3.1` (or `python/v0.3.1.post1`) | Python API, adapters and the embedded `tessera-order` `0.3.1` core |
| Repository | Full Git commit | All implementations, tests, docs and benchmarks together |

- Stable and unstable interfaces within one language share its package version.
- Bump only the packages whose shipped contents change. Changes to documentation or benchmarks alone do not release a library. When a Python-only adapter or packaging fix ships without a new Rust release, publish a PEP 440 post-release (`X.Y.Z.postN`, tag `python/vX.Y.Z.postN`) rather than drifting from the embedded Rust crate version.
- There is no shared algorithm version across independent reimplementations. Rust and C++ may run the same algorithm at different versions, or differ in optimizations for a while.
- An issue that affects several languages tracks each language's affected, fixed and validated status separately. Do not call it fixed in a language that has not been validated.
- Future Go modules are tagged by module directory, for example `go/tessera/v0.1.0` for `go/tessera/`.

## Compatibility

| Change | From 1.0 | During 0.x |
|---|---|---|
| Bug fix, or internal change that keeps every public contract | Patch | Patch |
| New public API or capability | Minor | Minor |
| Removed API or incompatible behavior | Major | Minor, with migration notes |

Cargo treats `0.3` to `0.4` as incompatible. The size of a performance change never decides the increment.

Public contracts are stability; float, NaN and signed-zero order; supported key and record types; record preservation after a panic or exception; and documented auxiliary-space bounds. Replacing the kernel behind an unstable interface is compatible only if that interface already left equal-key order unspecified and every other guarantee holds. The release notes name the kernel change.

## Release checklist

1. Confirm the language's [CI](docs/development/ci.md) checks are green at the release commit. Performance is accepted per pull request ([performance changes](CONTRIBUTING.md#performance-changes)), so a release needs no release-time benchmark run, comparison against the previous tag or first-release baseline. Optional `main` [performance tracking](docs/development/continuous-benchmarks.md#main-long-term-tracking) records the trend but does not gate a release.
2. Write the release notes, and update the README capability table. Collect the pull requests merged since the package's previous tag that affect the package. Check shared semantics and root configuration as well as the package directory. List what users can observe, grouped as breaking changes with their migration, additions, fixes and measured performance changes, each citing evidence that measured the claimed cases (usually the pull request's hosted verdict; a claim outside its cases cites the focused comparison that measured it, or is left out); leave out `ci`, `build`, `docs`, `refactor`, `test` and `chore` changes unless users see them. Link each entry to its pull request. The repository keeps no shared changelog file, avoiding conflicts between parallel PRs.
3. Before the first publication, tell Robert Clausecker that the project builds on his block partition.
4. Tag the release commit with the notes as the annotated tag message (`git tag -a rust/v0.3.1 --cleanup=whitespace -F notes.md`); git's default cleanup would strip Markdown headings as comments. Check `bun run --silent tools release notes --tag rust/v0.3.1` locally, then push the tag. Rust, Python and C++ workflows read the notes from the tag message before publishing and fail without publishing on a lightweight tag, a tag that no longer points at the triggering commit, or notes that are empty or contain no line starting with `#`, which is what git's default cleanup leaves of notes grouped under `##` headings. They then publish their package and a GitHub release with those notes; Rust releases become the repository's latest release; Python and C++ releases do not. Never move a published tag or replace a published version; fix a bad release with the next version.

Benchmark manifests record the implementation, package version, full commit, actual kernel, toolchain and build flags, and cross-language results state whether they compare one commit or each language's latest release. Uncommitted code also needs source hashes or a patch.

## Workflows

`release.yml` routes each tag to its language's release workflow, which runs that language's CI, checks the tag against the package metadata and reads the notes from the tag message; each tag has its own concurrency group. Rust and C++ publish from their reusable workflows. Python builds and tests its artifacts in `python-release.yml`, then the `python-publish` job in `release.yml` downloads those artifacts and notes, publishes to PyPI and creates the non-latest GitHub release. The PyPI publish job lives directly in `release.yml` so its OIDC `job_workflow_ref` matches the configured trusted publisher. [CI](docs/development/ci.md) owns check routing.

| Language | Active when | Release |
|---|---|---|
| Rust | Now. Publishes through crates.io [trusted publishing](https://crates.io/docs/trusted-publishing): the crate's settings name repository `Yikai-Liao/tessera-order` and workflow `release.yml`, so no registry token is stored | crates.io and a GitHub release with the `.crate` file and the tag's notes |
| Python | Now. Publishes through PyPI [trusted publishing](https://docs.pypi.org/trusted-publishers/) (`pypi` environment, `python-publish` job in `release.yml`), after `tools release verify` confirms `python/vX.Y.Z` (or `.postN`) matches the embedded Rust crate version | PyPI (`cp39`, `cp310`, `cp311-abi3` wheels across Linux x86-64/AArch64, macOS arm64/x86-64, Windows x64, plus sdist) and a non-latest GitHub release with the artifacts and tag notes |
| C++ | `cpp/CMakeLists.txt` exists, honoring `BUILD_TESTING`, registering CTest tests and configuring CPack with the package version | CI builds and runs CTest on Linux x86-64 and AArch64, macOS and Windows; the release checks `CPACK_PACKAGE_VERSION` against the tag and attaches a TGZ to a GitHub release with the tag's notes |

Until C++ exists, CI skips it and says so; no placeholder job passes without testing.
