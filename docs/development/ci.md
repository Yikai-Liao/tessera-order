# Continuous integration

[`ci.yml`](../../.github/workflows/ci.yml) decides which engine, comparison, benchmark and site checks a change needs, runs the repository-wide checks, and folds every result into the one required check, `ci-ok`. A language's checks are a reusable workflow that `ci.yml` calls only when its paths changed; [`ci_changes.ts`](../../.github/scripts/ci_changes.ts) holds the rules and its tests.

The router uses the merge base and the first matching row of the `ROUTES` table in `ci_changes.ts`, one row per path owner. Repository formatting, tooling type checks and tooling tests run on every change; additional jobs follow the changed boundary:

| Changed boundary | Additional automatic checks |
|---|---|
| Library source or library manifest | Rust validation and PR revision measurement |
| Library tests or Rust validation workflow | Rust validation |
| Root Cargo manifest/lockfile, toolchain or `.cargo/` | Rust validation, algorithm comparison checks and PR revision measurement |
| Comparator-only Rust, native adapters or algorithm-session tooling | Algorithm comparison checks |
| Shared benchmark Rust inputs, plan expansion, sampling or build configuration | Algorithm comparison checks and PR revision measurement |
| Revision execution, shard validation or local/continuous selection plans | PR revision measurement |
| Reports, status or publication tooling | Repository tooling checks only |
| Markdown, site inputs or shared figure templates | Docs |
| C++ library inputs | C++ validation, when its manifest exists |

An engine-only PR does not fetch third-party native sources, install TBB or run the external algorithm harness. Its Rust revision comparison is the `prepare → measure → report` pipeline; `Algorithm harness (Rust and C++)` (job ID `comparison`) tests the separate algorithm-comparison tool. In particular, adapter integration is no longer dynamically retested for every engine revision; library correctness and revision measurement remain the engine's automatic checks. Comparator-only changes do not rerun library platform, Miri or sanitizer jobs. Shared build inputs still select both consumers.

Issue and PR templates, Dependabot configuration, `fuzz/`, manual diagnostics and separate title/DCO/release workflows select no additional lane. A path that no row names, an unavailable base and manual CI dispatch select all lanes: an unclassified file is not known to be harmless, so a new kind of file needs a row. `ci.yml` is the only automatic task selector. PR updates cancel obsolete runs on the same PR. Every main push has an independent CI concurrency group, preserving a pending site deployment when a later non-docs push arrives.

## Lanes

Use a bounded number of lanes: the account has 20 concurrent slots shared with benchmarks and repository checks. Continuous measurement builds once and uses four measurement jobs; Rust validation keeps its independent platform, Miri and sanitizer jobs, and algorithm comparison runs only for its affected inputs. There is no benchmark-to-test dependency or account-wide priority/preemption guarantee; multiple PRs and other repositories can still consume the account. See [capacity](continuous-benchmarks.md#capacity-and-recovery).

| Check | Pull request | `main` |
|---|---|---|
| Repository-wide formatting, lints, type checks and CI scripts | `lint` | the same |
| Library tests | Linux stable and MSRV, macOS stable, Windows stable | all four platforms, stable and MSRV |
| Miri: serial backend and Rayon ownership fixtures | two parallel jobs | the same |
| AddressSanitizer | `asan` | the same |
| `cargo package` | in `rust / lint` | the same |
| Paired continuous measurement | one preparation job and four complete-case shards, inputs up to 1M | none |
| Algorithm harness tests, doctests, native smoke checks and report generation | `comparison`, for affected comparison/shared inputs | the same |

The Rust lint job checks all workspace features with Clippy and rustdoc, including the benchmark's native-facing Rust code. These commands do not link executables, so they set `TESSERA_BENCH_CHECK_ONLY=1` to skip the benchmark build script's native compilation and linker inputs. The lint runner needs neither fetched native sources nor native build tools. The comparison job sets the variable only for the Clippy and rustdoc step it owns, then compiles, links and exercises the native competitors, including the sequential-linkage check. Cargo tracks changes to the variable so a check-only build cannot suppress a later executable build's native setup.

The comparison job owns benchmark Rust formatting, Clippy, rustdoc and functional checks. It formats the workspace, checks the benchmark crate with every feature, and runs that crate's harness tests and doctests inside the isolated workspace that builds the native competitors. It checks the standalone native registry with GCC and Clang on Linux; a comparison-owned Windows job compiles it with both MSVC preprocessors without fetching competitors. Native benchmarking supports Linux x86-64; the library's Windows jobs no longer compile this C++ registry or initialize MSVC.

It pins Ubuntu 24.04 LTS and uses the image's Clang. It installs TBB development headers only when the package is absent, from a private source list naming the four Ubuntu suites on the Linux Kernel Archives mirror: hosted runners resolved the `ubuntu.com` DNS zone unreliably, and the other repositories the image configures neither provide TBB nor may gate its installation. Apt reads only that list and the image's keyrings, so it still verifies Ubuntu's signed metadata, while retries, fetch timeouts, a step timeout and the job timeout bound the installation.

The job uses a cached Cargo target and matching development profiles and alignment flags for its functional harness tests and executables. The cache retains workspace build outputs, including native objects, keyed by the [native build inputs](../design/benchmark-tooling.md#ownership) and the runner image version; pinned native sources are cached separately and revalidated by the existing fetcher. Algorithm comparisons and revision measurement still use optimized builds. The smoke check uses Rayon and the shared SIMD backend, validates the smoke suite and generates a partial report. The docs workflow checks algorithm ownership models and site types, then builds the shared Markdown sources and checks site links.

A Rust test cell's commands live in one composite action, [`rust-test`](../../.github/actions/rust-test/action.yml), which both the pull request cells and the extra `main` cells call. A job's `if` cannot branch on `matrix` — the matrix is expanded after the condition is evaluated — so the two sets of cells are two jobs over one action rather than one matrix with a skip per cell.

Each cell tests only `tessera-sort`, with the shared SIMD backend and optional `rayon` entry points, including on Rust 1.89. There are no separate scalar or no-std configurations. Rayon tests cover sharded root scatter, caller-pool bucket scheduling, the one-thread fallback, task panics and record preservation. The two Miri jobs interpret serial and Rayon ownership fixtures concurrently without dropping either configuration. The Rayon job additionally interprets sequential ownership fixtures for parallel storage and bucket handoff; native tests cover threaded scheduling. The AddressSanitizer job exercises the library's serial and Rayon builds; benchmark executables and competitor dependencies stay with the comparison job, which runs them unsanitized. No test lane is added for the continuous benchmark's two API groups.

The benchmark crate's own unit tests and doctests run with the algorithm harness. A library change does not rerun them: it still builds the smoke benchmark against both revisions, which rejects changed case identities, and the tested modules are benchmark-owned code.

The [benchmark diagnostics workflow](../../.github/workflows/bench-diagnostics.yml) runs only by manual dispatch. Select fixed-work counters or a five-layout sample and an optional baseline, using the four serial inputs in the diagnostics plan. Results and logs remain 14-day artifacts. It has read-only permissions, no Cloud submissions and a 45-minute timeout. Ordinary PRs and main pushes do not start these exploratory experiments.

## Adding a language

- Add a reusable workflow and a job in `ci.yml` that calls it, gated by `ci_changes.ts` so changes outside the language skip it.
- Give it one test cell per instruction set, not one per operating system. Put the cells every pull request runs in one job and the rest in a second job gated by `if: github.event_name != 'pull_request'`, both calling one composite action; [`rust-ci.yml`](../../.github/workflows/rust-ci.yml) and [its test action](../../.github/actions/rust-test/action.yml) show the pattern.
- Keep the pull request matrix at four cells or fewer. If it cannot shrink, cap it with `strategy: max-parallel` so the workflow queues its own cells instead of holding every slot.
- Report the language's checks into `ci-ok`; it is the aggregate CI check. DCO and PR-title checks are separate merge requirements.

## Reproducing locally

Run the [commands in `AGENTS.md`](../../AGENTS.md#commands) before pushing: a pull request also runs Miri, the MSRV, the benchmarks and one test cell per operating system, and `main` the remaining platform cells. The shared operations behind CI are CLI commands; workflows add only event attribution, runners, artifacts, credentials and publication:

| Task | Command |
|---|---|
| Reproduce change routing | `bun run --silent tools ci changes --base main --head HEAD` |
| Validate a PR title | `bun run --silent tools ci title 'fix(bench): validate source snapshots'` |
| Check every commit's DCO sign-off | `bun run --silent tools ci dco main HEAD` |
| Preview hosted benchmark jobs | `bun run --silent tools continuous matrix` |
| Reproduce the comparison checks | `bun run --silent tools ci check --native` |
| Fetch and verify pinned native sources | `bun run --silent tools native fetch` |
| Validate a release tag | `bun run --silent tools release verify --tag rust/v0.1.0` |

Benchmark commands are in the [protocol](benchmarking.md#command-capabilities). `ci check` is a functional smoke check (one-millisecond samples, not an experiment): it materializes HEAD and working-tree copies, runs harness tests and doctests, validates the smoke suite, measures one `u64/random` group, verifies that native programs link no parallel runtime under `--native`, and generates a partial report. The comparison job additionally formats the workspace and checks the benchmark crate with every feature. Routing, title and DCO have dependency-free bootstrap entry points in `.github/scripts/` that call the same functions; DCO runs the base commit's trusted code without installing PR code. Rust feature tests, Miri and sanitizers stay in their workflows and composite action.
