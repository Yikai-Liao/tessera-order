# Continuous integration

[`ci.yml`](../../.github/workflows/ci.yml) decides which engine, comparison, benchmark and site checks a change needs, runs the repository-wide checks, and folds every result into the one required check, `ci-ok`. A language's checks are a reusable workflow that `ci.yml` calls only when its paths changed; [`ci_changes.ts`](../../.github/scripts/ci_changes.ts) holds the rules and its tests.

The shared router uses the merge base. Engine changes select Rust validation, native functional checks and PR measurement. Continuous plan changes select measurement; native adapter changes select compilation and functional checks; report/status changes select tooling checks without platform tests, Miri, native compilation or timing. Markdown and site inputs select the reusable docs workflow. Unknown shared files and unavailable bases select all lanes conservatively. C++ checks activate when its manifest exists. `ci.yml` is the only automatic task selector. PR updates cancel obsolete runs on the same PR. Every main push has an independent CI concurrency group, preserving a pending site deployment when a later non-docs push arrives.

## Lanes

Use a bounded number of lanes: the account has 20 concurrent slots shared with benchmarks and repository checks. Continuous measurement builds once and uses four measurement jobs; ordinary Rust CI keeps its independent platform, comparison, Miri and sanitizer lanes. There is no benchmark-to-test dependency or account-wide priority/preemption guarantee; multiple PRs and other repositories can still consume the account. See [continuous capacity](continuous-benchmarks.md#capacity-and-recovery).

| Check | Pull request | `main` |
|---|---|---|
| Repository-wide formatting, lints, type checks and CI scripts | `lint` | the same |
| Rust tests | Linux stable and MSRV, macOS stable, Windows stable | all four platforms, stable and MSRV |
| Miri: serial backend and Rayon ownership fixtures | two parallel jobs | the same |
| AddressSanitizer | `asan` | the same |
| `cargo package` | in `rust / lint` | the same |
| Paired continuous measurement | one preparation job and four complete-case shards, inputs up to 1M | none |
| Native comparison smoke checks and report generation | `comparison`, using Clang | the same |

The Rust lint job checks all workspace features with Clippy and rustdoc, including the benchmark's native-facing Rust code. These commands do not link executables, so they set `TESSERA_BENCH_CHECK_ONLY=1` to skip the benchmark build script's native compilation and linker inputs. The lint runner needs neither fetched native sources nor native build tools. The comparison job leaves the variable unset and compiles, links and exercises the native competitors, including the sequential-linkage check. Cargo tracks changes to the variable so a check-only build cannot suppress a later executable build's native setup.

The comparison job uses a cached Cargo target and matching development profiles and alignment flags for its functional harness tests and executables, so it avoids optimizing the same generic Rust test and executable code repeatedly. Pinned native sources are cached and revalidated by the existing fetcher. Algorithm comparisons and revision measurement still use optimized builds. It executes comparison tests with Rayon and the shared SIMD backend, validates the smoke suite and generates a partial report. The docs workflow checks algorithm ownership models and site types, then builds the shared Markdown sources and checks site links.

A Rust test cell's commands live in one composite action, [`rust-test`](../../.github/actions/rust-test/action.yml), which both the pull request cells and the extra `main` cells call. A job's `if` cannot branch on `matrix` — the matrix is expanded after the condition is evaluated — so the two sets of cells are two jobs over one action rather than one matrix with a skip per cell.

Each cell tests the shared SIMD backend and optional `rayon` entry points, including on Rust 1.89. There are no separate scalar or no-std configurations. Rayon tests cover sharded root scatter, caller-pool bucket scheduling, the one-thread fallback, task panics and record preservation. The two Miri jobs interpret serial and Rayon ownership fixtures concurrently without dropping either configuration. The Rayon job additionally interprets sequential ownership fixtures for parallel storage and bucket handoff; native tests cover threaded scheduling. The AddressSanitizer job exercises serial and Rayon builds. No test lane is added for the continuous benchmark's two API groups.

The [benchmark diagnostics workflow](../../.github/workflows/bench-diagnostics.yml) runs only by manual dispatch. Select fixed-work counters or a five-layout sample and an optional baseline, using the four serial inputs in the diagnostics plan. Results and logs remain 14-day artifacts. It has read-only permissions, no Cloud submissions and a 45-minute timeout. Ordinary PRs and main pushes do not start these exploratory experiments.

## Adding a language

- Add a reusable workflow and a job in `ci.yml` that calls it, gated by `ci_changes.ts` so changes outside the language skip it.
- Give it one test cell per instruction set, not one per operating system. Put the cells every pull request runs in one job and the rest in a second job gated by `if: github.event_name != 'pull_request'`, both calling one composite action; [`rust-ci.yml`](../../.github/workflows/rust-ci.yml) and [its test action](../../.github/actions/rust-test/action.yml) show the pattern.
- Keep the pull request matrix at four cells or fewer. If it cannot shrink, cap it with `strategy: max-parallel` so the workflow queues its own cells instead of holding every slot.
- Report the language's checks into `ci-ok`; it is the aggregate CI check. DCO and PR-title checks are separate merge requirements.

## Reproducing locally

The commands are in [`AGENTS.md`](../../AGENTS.md#commands). Run them before pushing: a pull request runs Miri, the MSRV, the benchmarks and one test cell per operating system, and `main` also runs the remaining platform and MSRV cells.
