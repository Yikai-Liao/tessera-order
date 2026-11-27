---
layout: home

hero:
  name: Tessera
  text: Linear-time stable sorting, argsort, and selection in O(√n) auxiliary memory
  tagline: Sort records of any Rust type by fixed-width keys without Clone, Default, or an input-sized scratch buffer.
  image:
    src: /logo.svg
    alt: Tessera logo, a mosaic rosette
  actions:
    - theme: brand
      text: Get started
      link: /guide/getting-started
    - theme: alt
      text: Benchmarks
      link: /benchmarks/implementations
    - theme: alt
      text: How it works
      link: /explain/
    - theme: alt
      text: GitHub
      link: https://github.com/Yikai-Liao/tessera-order
---

<!-- SPDX-License-Identifier: BSD-2-Clause -->

<script setup lang="ts">
import ImplementationComparison from "./.vitepress/theme/components/ImplementationComparison.vue";
</script>

<ImplementationComparison view="headlines" />

## Compare at a glance

| Property | **Tessera** | `slice::sort` (std) | `radsort` | `glidesort` |
|---|---|---|---|---|
| **Time complexity** | $O(n)$ (fixed-width key) | $O(n \log n)$ | $O(n)$ (fixed-width key) | $O(n \log n)$ |
| **Extra memory (serial sort)** | $O(\sqrt{n})$ block scratch | $O(n)$ | $O(n)$ | $O(n)$ (adaptive) |
| **Stable** | Yes | Yes | Yes | Yes |
| **Key contract** | Fixed-width `RadixKey` (`u*`, `i*`, `f*`, `[u8; 1..=16]`, tuples, `Reverse`) | Any `Ord` or comparator | Fixed-width `Key` | Any `Ord` or comparator |
| **Record bounds** | None (moves any `T`; no `Copy`/`Clone`/`Default`) | None | None | None |
| **Parallel (`rayon`)** | `par_sort*`, `par_argsort*` ($O(P\sqrt{n})$ extra) | Separate (`rayon::par_sort`) | No | No |
| **Operations** | `sort*`, `sort_by_cached_key`, `argsort*`, `select_nth*` | `sort*`, unstable `select_nth` | `sort`, `sort_by_key` | `sort*` |

## One engine, zero record-trait bounds

<div class="tsr-home-grid">
<div class="tsr-card">

### `tessera-order` 0.2

```rust
use std::cmp::Reverse;

let mut rows = vec![(30i64, "c"), (10, "a"), (30, "d")];
tessera_order::sort_by_key(&mut rows, |r| r.0);
assert_eq!(rows, [(10, "a"), (30, "c"), (30, "d")]);

// Sorted indices descending by key without moving records:
let order = tessera_order::argsort_by_key(&rows, |r| Reverse(r.0));
assert_eq!(order, [1, 2, 0]);

// Exact stable rank selection without sorting the rest:
let (_, median, _) = tessera_order::select_nth_by_key(&mut rows, 1, |r| r.0);
assert_eq!(*median, (30, "c"));
```

Any supported fixed-width key works out of the box (see [key encoding](../conformance/semantics.md#key-encoding)). Equal keys always preserve input order, and every original record stays in the slice exactly once even if the key function panics.

</div>
<div class="tsr-card tsr-fit-stack">
<div class="tsr-fit-block is-good">

### Good fit

- Large slices keyed by integers, floats, fixed-width byte arrays, or composite tuples.
- Workloads that require stability without paying for an $O(n)$ auxiliary buffer.
- Moving wide records is expensive (`argsort`) or only one ranked element is needed (`select_nth`).
- Real-world inputs with sorted runs, reversed runs, narrow bit spans, or heavily repeated keys.

</div>
<div class="tsr-fit-block">

### Use something else

- **Comparators or variable-length keys** (strings, slices): use `slice::sort_by`.
- **No allocator / `no_std`**: Tessera requires `std` and heap scratch.
- **Unstable `u32` keys** where reordering equal keys is acceptable: unstable SIMD sorts can be faster.

</div>
</div>
</div>

## Measured against other sorts

<ImplementationComparison view="overview" />

Next: [Getting started](guide/getting-started.md) · [vs Other Sorts](benchmarks/implementations.md) · [How it works](explain/index.md) · [Ordering semantics](../conformance/semantics.md) · [Repository task index](README.md)
