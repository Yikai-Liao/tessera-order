# Continuous integration

[`ci.yml`](../../.github/workflows/ci.yml) decides which language checks a change needs, runs the repository-wide checks, and folds every result into the one required check, `ci-ok`. A language's checks are a reusable workflow that `ci.yml` calls only when its paths changed; [`ci_changes.py`](../../.github/scripts/ci_changes.py) holds the rules and its tests.

## Lanes

Every job holds one of the account's concurrent job slots, and a free account runs 20 at once. A pull request that touches Rust peaked at 21 jobs before the split below and peaks at 15 now, counting every workflow it triggers: 12 from `CI`, one benchmark build, `DCO` and `PR title`. A language fits into a fixed number of lanes, not one per operating system and compiler version, because the full matrices Rust, C++ and Python already declare would need 28 test cells, plus the lint, Miri and sanitizer jobs each of them adds.

| Check | Pull request | `main` |
|---|---|---|
| Repository-wide formatting, lints, type checks and CI scripts | `lint` | the same |
| Rust tests | Linux stable and MSRV, macOS stable, Windows stable | all four platforms, stable and MSRV |
| Miri: std, no-std, Portable SIMD | three jobs | the same |
| AddressSanitizer | `asan` | the same |
| `cargo package` and the no-std target | in `rust / lint` | the same |
| Benchmark build | one job, both configurations | the same |

The split keeps `main` at full coverage, which no pull request waits for, and keeps the pull request path short. Its longest jobs are AddressSanitizer and Miri's std suite, about 90 seconds to two minutes each; both are one runner each, so shortening them further means interpreting or testing less, not adding runners.

A cell's commands live in one composite action, [`rust-test`](../../.github/actions/rust-test/action.yml), which both the pull request cells and the extra `main` cells call. A job's `if` cannot branch on `matrix` — the matrix is expanded after the condition is evaluated — so the two sets of cells are two jobs over one action rather than one matrix with a skip per cell.

Each cell tests the optional `rayon` entry points, including on Rust 1.85. Stable Linux also tests all features together, so Rayon and Portable SIMD are exercised in one build; stable macOS and Windows compile that combination. The Rayon entry points currently use the serial engine, so their tests check the API and fallback contracts rather than task scheduling.

## Adding a language

- Add a reusable workflow and a job in `ci.yml` that calls it, gated by `ci_changes.py` so changes outside the language skip it.
- Give it one test cell per instruction set, not one per operating system. Put the cells every pull request runs in one job and the rest in a second job gated by `if: github.event_name != 'pull_request'`, both calling one composite action; [`rust-ci.yml`](../../.github/workflows/rust-ci.yml) and [its test action](../../.github/actions/rust-test/action.yml) show the pattern.
- Keep the pull request matrix at four cells or fewer. If it cannot shrink, cap it with `strategy: max-parallel` so the workflow queues its own cells instead of holding every slot.
- Report the language's checks into `ci-ok`; it is the only required check.

## Reproducing locally

The commands are in [`AGENTS.md`](../../AGENTS.md#commands). Run them before pushing: a pull request runs Miri, the MSRV, the benchmarks and one test cell per operating system, and `main` also runs the remaining platform and MSRV cells.
