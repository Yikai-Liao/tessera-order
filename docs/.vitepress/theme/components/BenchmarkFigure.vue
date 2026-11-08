<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { computed } from "vue";
import { type RowChart, renderRows } from "../../../../bench/plot/rows.ts";

const props = defineProps<{ chart: RowChart }>();
const source = computed(
  () => `data:image/svg+xml;charset=utf-8,${encodeURIComponent(renderRows(props.chart))}`,
);
</script>

<template>
  <figure class="benchmark-figure">
    <img :src="source" :alt="chart.title" />
  </figure>
</template>

<style scoped>
.benchmark-figure {
  margin: 24px 0;
}
.benchmark-figure img {
  display: block;
  width: 100%;
  height: auto;
}
</style>
