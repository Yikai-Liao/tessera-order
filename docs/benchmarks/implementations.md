---
aside: false
pageClass: performance-page
---

# Implementation comparison

<script setup lang="ts">
import ImplementationComparison from "../.vitepress/theme/components/ImplementationComparison.vue";
</script>

How Tessera compares with other Rust and C++ implementations of the same operation on the same inputs, measured on Bencher's Intel v1 Bare Metal runners. Each operation has its own section: full sorting, argsort, selection and category partitioning. Each implementation's latest run is shown; Tessera runs on every relevant main push and opponents when dispatched.

<ImplementationComparison />

## Reading the figures

**Speed** is the section's reference run time divided by the implementation's run time on the same case, so 2× takes half as long. The reference is what the standard library offers for the result, such as `slice::sort` for full sorting and `select_nth_unstable` for selection. A row names the call an implementation makes when its name alone would not, such as a full sort standing in for selection. Speeds in different sections have different references and do not compare with each other. A ranking is the geometric mean over the input patterns named in its subtitle, at one length; an implementation that lacks one of those cases is not ranked rather than credited. The length figure follows random input across lengths; the pattern figures compare input patterns at one length, and the short-input figure every pattern at shorter lengths, where fixed per-call costs decide. **Extra memory** is the largest peak heap allocation of one call beyond the input, including an output it returns such as argsort's indices, over the named cases; "–" means it was not recorded.

Runs of different implementations are independent: their ratios are descriptive and come from different jobs, possibly days apart, on machines of the same specification. They are not the paired verdicts PR checks give; [main history](index.md) shows Tessera's own trend. Coverage and publication belong to the [tracking contract](../development/continuous-benchmarks.md#data-and-site-publication).
