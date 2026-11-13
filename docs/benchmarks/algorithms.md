---
aside: false
pageClass: performance-page
---

# Algorithm comparison

<script setup lang="ts">
import AlgorithmComparison from "../.vitepress/theme/components/AlgorithmComparison.vue";
</script>

How Tessera compares with other Rust and C++ sorts on the same inputs, measured on Bencher's Intel v1 Bare Metal runners. Each sort's latest run is shown; Tessera runs on every relevant main push and opponents when dispatched.

<AlgorithmComparison />

## Reading the figures

**Speed** is `slice::sort` run time divided by the sort's run time on the same case, so 2× takes half as long. A ranking is the geometric mean over the input patterns named in its subtitle, at one length; a sort that lacks one of those cases is not ranked rather than credited. **Extra memory** is the largest peak heap allocation of one sort beyond the input, over the named cases; "–" means it was not recorded.

Runs of different sorts are independent: their ratios are descriptive and come from different jobs, possibly days apart, on machines of the same specification. They are not the paired verdicts PR checks give; [main history](index.md) shows Tessera's own trend. Coverage and publication belong to the [tracking contract](../development/continuous-benchmarks.md#data-and-site-publication).
