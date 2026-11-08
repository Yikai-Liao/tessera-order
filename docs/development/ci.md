# Continuous integration

[`ci.yml`](../../.github/workflows/ci.yml) decides which language checks a change needs, runs the repository-wide checks, and folds every result into the one required check, `ci-ok`. A language's checks are a reusable workflow that `ci.yml` calls only when its paths changed; [`ci_changes.py`](../../.github/scripts/ci_changes.py) holds the rules and its tests.

Language files select their language; Rust also selects Python because the planned binding embeds the core. Shared contracts and unknown paths select all implemented languages. Site tooling, CI scripts and Markdown outside language/conformance directories select none. The router uses the merge base; unavailable bases and manual/new-branch runs select everything. Python and C++ checks activate only when their manifests exist.

## Lanes

Use a bounded number of lanes: the account has 20 concurrent slots, shared with benchmark, DCO and title checks. Full language/platform/compiler Cartesian products would exhaust that budget.

| Check | Pull request | `main` |
|---|---|---|
| Repository-wide formatting, lints, type checks and CI scripts | `lint` | the same |
| Rust tests | Linux stable and MSRV, macOS stable, Windows stable | all four platforms, stable and MSRV |
| Miri: std, no-std, Portable SIMD | three jobs | the same |
| AddressSanitizer | `asan` | the same |
| `cargo package` and the no-std target | in `rust / lint` | the same |
| Benchmark build | one job, both configurations | the same |
| Native comparison smoke checks and report generation | `comparison`, using Clang | the same |

The Rust lint job checks all workspace features with Clippy and rustdoc, including the benchmark's native-facing Rust code. These commands do not link executables, so they set `TESSERA_BENCH_CHECK_ONLY=1` to skip the benchmark build script's native compilation and linker inputs. The lint runner needs neither fetched native sources nor native build tools. The comparison job leaves the variable unset and compiles, links and exercises the native competitors, including the sequential-linkage check. Cargo tracks changes to the variable so a check-only build cannot suppress a later executable build's native setup.

The comparison job builds the pinned native competitors, validates the smoke suite and generates a partial report. The docs workflow checks shared task skills and renders their sources as pages; it does not require a retrieval CLI.

A cell's commands live in one composite action, [`rust-test`](../../.github/actions/rust-test/action.yml), which both the pull request cells and the extra `main` cells call. A job's `if` cannot branch on `matrix` — the matrix is expanded after the condition is evaluated — so the two sets of cells are two jobs over one action rather than one matrix with a skip per cell.

## Adding a language

- Add a reusable workflow and a job in `ci.yml` that calls it, gated by `ci_changes.py` so changes outside the language skip it.
- Give it one test cell per instruction set, not one per operating system. Put the cells every pull request runs in one job and the rest in a second job gated by `if: github.event_name != 'pull_request'`, both calling one composite action; [`rust-ci.yml`](../../.github/workflows/rust-ci.yml) and [its test action](../../.github/actions/rust-test/action.yml) show the pattern.
- Keep the pull request matrix at four cells or fewer. If it cannot shrink, cap it with `strategy: max-parallel` so the workflow queues its own cells instead of holding every slot.
- Report the language's checks into `ci-ok`; it is the only required check.

## Reproducing locally

The commands are in [`AGENTS.md`](../../AGENTS.md#commands). Run them before pushing: a pull request runs Miri, the MSRV, the benchmarks and one test cell per operating system, and `main` also runs the remaining platform and MSRV cells.
