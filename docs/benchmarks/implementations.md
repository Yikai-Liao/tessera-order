---
pageClass: performance-page
---

# Implementation comparison

<script setup lang="ts">
import ImplementationComparison from "../.vitepress/theme/components/ImplementationComparison.vue";
</script>

How Tessera compares with other Rust and C++ implementations of the same operation, on the same inputs and the same kind of machine. The summary gives each operation's main result; the sections after it break each result down by input length, input pattern, element type and memory.

<ImplementationComparison />

## Reading the figures

**Speed** is the section's reference run time divided by the implementation's run time on the same case, so 2× takes half as long. Each figure names its reference: for serial calls, what the standard library offers for the same stable result, such as `slice::sort` for full sorting and a full `slice::sort` for selection, since the standard library has no stable selection. A row names the call an implementation makes when its name alone would not, such as a full sort standing in for selection. Speeds in different sections have different references and do not compare with each other. A ranking is the geometric mean over the input patterns named in its subtitle, at one length; an implementation that lacks one of those cases is not ranked rather than credited. The length figure follows random input across lengths; the pattern figures compare input patterns at one length, and the short-input figure every pattern at shorter lengths, where fixed per-call costs decide. **Extra memory** is the largest peak heap allocation of one call beyond the input, including an output it returns such as argsort's indices, over the named cases; "–" means it was not recorded.

Runs of different implementations are independent: their ratios are descriptive and come from different jobs, possibly days apart, on machines of the same specification. They are not the paired verdicts PR checks give; [main history](index.md) shows Tessera's own trend. Coverage and publication belong to the [tracking contract](../development/continuous-benchmarks.md#data-and-site-publication).
