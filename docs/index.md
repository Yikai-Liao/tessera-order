---
layout: home

hero:
  name: Tessera
  text: Stable radix sorting in O(√n) extra memory
  tagline: A Rust library that sorts records of any type by an integer, float or fixed-width tuple key, keeps equal keys in input order, and needs no input-sized buffer.
  image:
    src: /logo.svg
    alt: Tessera logo, a mosaic rosette
  actions:
    - theme: brand
      text: Get started
      link: /guide/getting-started
    - theme: alt
      text: How it works
      link: /explain/
    - theme: alt
      text: Benchmarks
      link: /benchmarks/implementations
    - theme: alt
      text: GitHub
      link: https://github.com/Yikai-Liao/tessera-order

features:
  - title: Small memory footprint
    details: Scratch grows as √n, not with the input. A textbook radix sort needs a second buffer as large as the input.
    link: /explain/block-partition
  - title: Free when the data is easy
    details: Sorted, reversed, nearly sorted and constant inputs skip the radix passes. A heavily repeated key takes one partition, and key bits that never change are never sorted.
    link: /explain/routing
  - title: Safe for any record
    details: Records need no Copy, Clone, Default or Ord. Each is moved, never duplicated, even if the key function panics.
    link: /explain/safety
  - title: One engine, several answers
    details: Sort in place, return sorted indices without moving records, or find one ranked record without sorting the rest.
    link: /guide/getting-started#choose-an-operation
---

<!-- SPDX-License-Identifier: BSD-2-Clause -->

<script setup lang="ts">
import ImplementationComparison from "./.vitepress/theme/components/ImplementationComparison.vue";
</script>

## In three lines

```rust
let mut rows = vec![(30i64, "c"), (10, "a"), (30, "d")];
tessera_order::sort_by_key(&mut rows, |row| row.0);
assert_eq!(rows, [(10, "a"), (30, "c"), (30, "d")]);
```

The key is an integer up to 128 bits, an `f32` or `f64`, `Reverse` of a supported key, or a fixed-width tuple of supported keys ([key encoding](../conformance/semantics.md#key-encoding) for limits). Records with equal keys keep their input order. The published 0.1 release names this function `stable_sort_by_key` and does not support tuple keys; see [versions](guide/getting-started.md#versions).

## Is it for you?

**A good fit** when you sort large slices by a numeric or fixed-width tuple key, need equal keys to stay in input order, or cannot afford a second buffer as large as the input.

**Use something else** when the order comes from a comparator, or keys are strings or other variable-length values: use `slice::sort_by`. Tessera also needs an allocator.

## Measured against other sorts

<ImplementationComparison view="overview" />

[Use the library](guide/getting-started.md), [learn the algorithm](explain/index.md), or [choose a development task](README.md).
