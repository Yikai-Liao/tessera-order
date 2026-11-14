---
layout: home

hero:
  name: Tessera
  text: Stable radix sort with O(√n) auxiliary memory
  tagline: Linear time, adaptive to real data, and safe for records that own resources.
  image:
    src: /logo.svg
    alt: Tessera logo, a mosaic rosette
  actions:
    - theme: brand
      text: How it works
      link: /explain/
    - theme: alt
      text: Get started
      link: /guide/getting-started
    - theme: alt
      text: GitHub
      link: https://github.com/Yikai-Liao/tessera-order

features:
  - title: Stable, without a second buffer
    details: A block partition recycles consumed input blocks as output blocks, so a split needs O(√n) memory instead of an n-record buffer.
    link: /explain/block-partition
  - title: Adaptive
    details: Sorted, reversed, nearly sorted, constant and heavily repeated inputs take shortcuts, and key bits that never change cost nothing.
    link: /explain/routing
  - title: Any record type
    details: No Copy, Clone, Default or Ord. Records are moved, never duplicated, even if the key function misbehaves or panics.
    link: /explain/safety
  - title: Wide keys
    details: Integers up to 128 bits, f32 and f64 in total_cmp order, and Reverse for descending order that stays stable.
    link: /explain/keys
---

<!-- SPDX-License-Identifier: BSD-2-Clause -->

<script setup lang="ts">
import AlgorithmComparison from "./.vitepress/theme/components/AlgorithmComparison.vue";
</script>

## Measured against other sorts

<AlgorithmComparison view="overview" />

[Use the library](guide/getting-started.md), [learn the algorithm](explain/index.md), or [choose a development task](README.md).
