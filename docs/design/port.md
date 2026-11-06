# Rust engine port

The engine in `rust/tessera-sort` is a Stable Rust port of the research
prototype in `sort-research-rs` at commit
`6c0877f0491dc3b8fc052aa529bce44bf8a53fa8`. The crate is unreleased
(`0.0.0`, `publish = false`).

## What changed

The sorting pipeline is unchanged: order probe, budgeted insertion, variation
scan, root narrowing for 128-bit keys, node routing, digit planning, tile
leaves, order confirmation and stable reversal of equal-key runs all follow
the prototype.

The port changes how the engine is reached, not what it decides:

| Prototype | Port |
|---|---|
| Nightly specialization selects the record path | `Extract` trait with two implementations: `ByKey` for user key functions and `Direct` for `stable_sort` |
| `Copy` records skip cached bucket IDs and initialize scratch on the first pass | `stable_sort_by_key` always caches bucket IDs; `stable_sort` recomputes library-encoded keys |
| Open `Key` trait | Sealed `RadixKey` for primitive integers, `f32`, `f64` and `Reverse<K>` |
| Wide-key narrowing casts the record slice | Narrowing wraps the extractor (`KeyHalf`) |
| Scratch owned by each call | Optional `Workspace<T>` keeps capacity between calls |

Free functions create their scratch only after the root fast paths, so
ordered and constant inputs do not allocate. Block metadata is created only
when a node needs a block partition.

## Correctness

The tests are written for this repository and use only the public API and
engine internals. They cover every key encoding, float edge cases, thresholds
around the insertion limit and tile size, 128-bit key halves, over-aligned and
zero-sized records, owned records (`String`, drop tracking) and `&mut`
records under panicking and changing key functions, workspace reuse after a
panic, and allocation counts of a warm workspace.

Run locally on x86-64 Linux: Stable (debug and release), Rust 1.85, Miri,
AddressSanitizer, Clippy, rustdoc and `cargo package`. The multi-platform
GitHub CI has not run on this code yet.

## Performance

The prototype needs Nightly specialization, so the port was measured against
it inside one Nightly binary of the research harness (`radix_ab`). This
isolates the source port; it is not a Stable-vs-Nightly comparison. The
harness and the raw data stay outside this repository because the harness is
Apache-2.0 code.

Method: fixed seed, one pinned core, variant order rotated every round, input
restored outside the timed region, every output checked against a stable
reference, scratch allocation included in the time. Each case reports the
median over rounds of `port / mean(prototype A1, prototype A2)`. The same
binary also contains:

- a second copy of the prototype in another crate, to expose code-placement
  effects;
- the prototype forced onto cached bucket IDs, to separate the planned caching
  policy from the port itself.

| Matrix | API | Cases | Processes × rounds | Median ratio | Range |
|---|---|---:|---|---:|---|
| Default build, small sizes | `stable_sort_by_key` | 105 | 3 × 21 | 0.984 | 0.58–1.20 |
| Default build, small sizes | `stable_sort` | 63 | 3 × 21 | 0.995 | 0.62–1.11 |
| Default build, full matrix | `stable_sort_by_key` | 136 | 1 × 9 | 1.004 | 0.53–1.14 |
| Default build, full matrix | `stable_sort` | 92 | 1 × 9 | 0.990 | 0.60–1.23 |
| `target-cpu=native`, fat LTO | `stable_sort_by_key` | 45 | 1 × 9 | 1.001 | 0.81–1.12 |
| `target-cpu=native`, fat LTO | `stable_sort` | 27 | 1 × 9 | 0.994 | 0.84–1.07 |

The default build is the harness's thin-LTO profile. Medians weigh every case
equally; they are not an application-level speedup.

Eight of the slowest cases were rerun with 3 processes × 21 rounds. The
cases that stayed outside the control range:

| Case | API | Ratio | Port / cached prototype |
|---|---|---:|---:|
| `record128` zipf 1.3, n = 128 | by key | 1.168 | 1.030 |
| `u128` 256 distinct, n = 48 | by key | 1.121 | 1.028 |
| `record128` 256 distinct, n = 128 | by key | 1.113 | 1.017 |
| `record128` local random, n = 100,000 | by key | 1.067 | 1.004 |
| `u128` descending, n = 100,000 | keys | 1.275 | 1.005 |
| `u128` 256 distinct, n = 48 | keys | 1.088 | 1.021 |

The `stable_sort_by_key` cases stay within 3% of the prototype forced onto
cached bucket IDs. Those differences come from the caching policy that the
port adopts on purpose, plus code placement.

`u128` descending with `stable_sort` was the only rerun case that was slower
in almost every round (60 of 63). A descending input
takes only the order probe and the stable reversal, and the machine code of
`stable_sort::<u128>` is identical to that of `stable_sort_by_key::<u128>`,
which is not slower. Its instructions differ only in addresses. In a copy of
the binary, redirecting the one call into the `stable_sort::<u128>` engine
body to the `stable_sort_by_key::<u128>` body lowered the ratio from
1.19–1.41 to 0.99–1.05 across three alternating processes. The gap is
therefore caused by code placement, not by the port.

The same intervention explains the largest gaps in either direction on
ordered `u32` inputs (0.53–1.01 before, 0.98–1.01 after redirecting six
instruction-identical probe functions to one address). Neither experiment
identifies which front-end mechanism is responsible.

## Not yet done

- The standalone benchmark runner, shared input vectors and the release
  report described in [benchmarking.md](../development/benchmarking.md).
- Multi-platform CI on GitHub.
- A crates.io release. See [RELEASING.md](../../RELEASING.md).
