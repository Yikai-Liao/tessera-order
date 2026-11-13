# Continuous integration

[`ci.yml`](../../.github/workflows/ci.yml) decides which engine, comparison, benchmark and site checks a change needs, runs the repository-wide checks and folds every result into the one required check, `ci-ok`. Each language's checks are a reusable workflow called only when its paths changed. [`ci_changes.ts`](../../.github/scripts/ci_changes.ts) holds the rules and their tests: the router diffs against the merge base and takes the first matching row of its `ROUTES` table, one row per path owner. Formatting, tooling type checks and tooling tests run on every change; other jobs follow the changed boundary:

| Changed boundary | Additional automatic checks |
|---|---|
| Library source or library manifest | Rust validation, PR revision measurement and Docs (the figures replay the engine) |
| Library tests or Rust validation workflow | Rust validation |
| Root Cargo manifest/lockfile, toolchain or `.cargo/` | Rust validation, algorithm comparison checks and PR revision measurement |
| Native adapters or algorithm-session tooling | Algorithm comparison checks |
| Shared benchmark Rust inputs, adapter kernels, plan selection/expansion, sampling or build configuration | Algorithm comparison checks and PR revision measurement |
| Revision execution or shard validation | PR revision measurement |
| Reports, status or publication tooling | Repository tooling checks only |
| Markdown, site inputs, shared figure templates or the figure engine (`rust/tessera-trace/`) | Docs |
| C++ library inputs | C++ validation, when its manifest exists |

An engine-only PR fetches no native sources, installs no TBB and skips the algorithm harness (`Algorithm harness (Rust and C++)`, job ID `comparison`): its automatic checks are library correctness and the `prepare → measure → report` revision measurement. Comparator-only changes skip the library platform, Miri and sanitizer jobs. Shared build inputs select both.

Issue and PR templates, Dependabot configuration, `fuzz/`, manual diagnostics and the separate title, DCO and release workflows select no extra lane. A path no row names, an unavailable base and manual dispatch select all lanes, so a new kind of file needs a row. The opt-in [main performance workflow](continuous-benchmarks.md#main-long-term-tracking) is independent of these lanes and `ci-ok`. PR updates cancel obsolete runs; each main push has its own concurrency group, so a later non-docs push cannot cancel a pending site deployment.

## Lanes

Keep lanes bounded: the account's 20 concurrent slots are shared by benchmarks, checks, other PRs and other repositories, with no priority or preemption. Continuous measurement builds once and takes its matrix width from [`CONTINUOUS_SHARDS`](../../bench/continuous/paired.ts); no test waits for it ([capacity](continuous-benchmarks.md#capacity-and-recovery)).

| Check | Pull request | `main` |
|---|---|---|
| Repository-wide formatting, lints, type checks and CI scripts | `lint` | the same |
| Library tests | Linux stable and MSRV, macOS stable, Windows stable | all four platforms, stable and MSRV |
| Miri: serial backend and Rayon ownership fixtures | two parallel jobs | the same |
| AddressSanitizer | `asan` | the same |
| `cargo package` | in `rust / lint` | the same |
| Paired continuous measurement | one preparation job and the configured complete-case shards, inputs up to 1M | none; independent opt-in absolute-time tracking on relevant pushes |
| Algorithm harness tests, doctests, native smoke checks and report generation | `comparison`, for affected comparison/shared inputs | the same |

### Rust tests

Each test cell tests only `tessera-order`, with the SIMD backend and the `rayon` entry points, also on Rust 1.89. There are no scalar or no-std configurations. The cells disable test-profile debug information to save compilation, at the cost of less informative backtraces; Miri, sanitizers, benchmarks and release builds are unaffected. Rayon tests cover sharded root scatter, caller-pool bucket scheduling, the one-thread fallback, task panics and record preservation. Miri uses [bounded safety fixtures](../../CONTRIBUTING.md#validation), leaving exhaustive arithmetic to native tests. The two Miri jobs interpret the serial and Rayon ownership fixtures, the latter including sequential fixtures for parallel storage and bucket handoff; native tests cover threaded scheduling. AddressSanitizer runs the library's serial and Rayon builds; benchmark executables stay with the comparison job, unsanitized.

A cell's commands live in one composite action, [`rust-test`](../../.github/actions/rust-test/action.yml), called by both the pull request cells and the extra `main` cells. They are two jobs over one action because a job's `if` is evaluated before its matrix expands.

The Rust lint job runs Clippy and rustdoc over all workspace features, including the benchmark's native-facing code. These commands link nothing, so they set `TESSERA_BENCH_CHECK_ONLY=1` to skip native compilation and linker inputs, and the lint runner needs no native sources or tools. Cargo tracks the variable, so a check-only build cannot suppress a later executable build's native setup.

### Algorithm harness

The comparison job owns benchmark formatting, Clippy, rustdoc and functional checks. It checks the benchmark crate with every feature (setting `TESSERA_BENCH_CHECK_ONLY` only for Clippy and rustdoc), runs its harness tests and doctests in the isolated workspace that builds the native competitors, and checks that native programs link no parallel runtime. It compiles the standalone native registry with GCC and Clang on Linux and, in a comparison-owned Windows job, with both MSVC preprocessors; native benchmarking itself supports Linux x86-64 only. The smoke check uses Rayon, exercises the shared Criterion/paired executable with tiny random/equal inputs for every available production algorithm and record type (including normal memory evidence for `glidesort-sqrt`), validates the smoke suite and generates a partial report. A library change does not rerun the benchmark crate's own tests: building the smoke benchmark against both revisions already rejects changed case identities.

The job pins Ubuntu 24.04 LTS and the image's Clang. It installs TBB headers only when absent, from a private source list naming the four Ubuntu suites on the Azure archive mirror, which [recovered from the Canonical outage](https://github.com/actions/runner-images/issues/15296) where the Kernel.org mirror timed out. Apt reads only that list and the image's keyrings, so it still verifies Ubuntu's signed metadata, and unrelated runner repositories cannot block it; retries and step and job timeouts bound the installation.

A cached Cargo target keeps workspace outputs, including native objects, keyed by the [native build inputs](../design/benchmark-tooling.md#ownership) and the runner image version; pinned native sources are cached separately and revalidated by the fetcher. Functional checks use development profiles with the benchmark alignment flags; comparisons and revision measurement use optimized builds.

### Docs and diagnostics

The docs workflow builds the figure engine for WebAssembly, tests the library's `trace` hooks, lints the engine crate, runs the model and replay tests and site type checks, then builds the site and checks its links.

The [benchmark diagnostics workflow](../../.github/workflows/bench-diagnostics.yml) runs only by manual dispatch: fixed-work counters or a five-layout sample on the four serial inputs of the diagnostics plan, with an optional baseline. It has read-only permissions, no Cloud submission and a 45-minute timeout, and keeps results and logs as 14-day artifacts.

## Optional faster local tests

For a local correctness iteration, disable test-profile debug information for one command:

```sh
CARGO_PROFILE_TEST_DEBUG=0 cargo test -p tessera-order --lib --tests --features rayon
```

Backtraces may be less informative. Still run the normal validation commands, and never apply this to benchmark or release builds.

## Adding a language

- Add a reusable workflow and a job in `ci.yml` that calls it, gated by `ci_changes.ts` so changes outside the language skip it.
- Give it one test cell per instruction set, not one per operating system. Put the cells every pull request runs in one job and the rest in a second job gated by `if: github.event_name != 'pull_request'`, both calling one composite action; [`rust-ci.yml`](../../.github/workflows/rust-ci.yml) and [its test action](../../.github/actions/rust-test/action.yml) show the pattern.
- Keep the pull request matrix at four cells or fewer. If it cannot shrink, cap it with `strategy: max-parallel` so the workflow queues its own cells instead of holding every slot.
- Report the language's checks into `ci-ok`; it is the aggregate CI check. DCO and PR-title checks are separate merge requirements.

## Reproducing locally

Run the [commands in `AGENTS.md`](../../AGENTS.md#commands) before pushing; a pull request adds Miri, the MSRV, the benchmarks and one test cell per operating system, and `main` the remaining platform cells. CI's operations are CLI commands; workflows add only event attribution, runners, artifacts, credentials and publication:

| Task | Command |
|---|---|
| Reproduce change routing | `bun run --silent tools ci changes --base main --head HEAD` |
| Validate a PR title | `bun run --silent tools ci title 'fix(bench): validate source snapshots'` |
| Check every commit's DCO sign-off | `bun run --silent tools ci dco main HEAD` |
| Preview hosted benchmark jobs | `bun run --silent tools continuous matrix` |
| Reproduce the comparison checks | `bun run --silent tools ci check --native` |
| Fetch and verify pinned native sources | `bun run --silent tools native fetch` |
| Validate a release tag | `bun run --silent tools release verify --tag rust/v0.1.0` |

Benchmark commands are in the [protocol](benchmarking.md#command-capabilities). `ci check` is a functional smoke check with one-millisecond samples, not an experiment: it materializes HEAD and working-tree copies, runs harness tests and doctests, validates the smoke suite, measures one `u64/random` group, checks native linkage under `--native` and generates a partial report. Routing, title and DCO also have dependency-free entry points in `.github/scripts/`; DCO runs the base commit's trusted code without installing PR code.
