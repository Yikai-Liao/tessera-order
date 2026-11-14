<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { withBase } from "vitepress";
import { computed, onMounted, shallowRef, useId } from "vue";
import { renderRows } from "../../../../bench/plot/rows.ts";
import { type ComparisonData, comparisonSchema } from "../../../../bench/tracking/result.ts";

// `overview` shows the headline ranking with a link to the full page; `all` shows every figure.
const props = withDefaults(defineProps<{ view?: "overview" | "all"; source?: string }>(), {
  view: "all",
  source:
    "https://raw.githubusercontent.com/Yikai-Liao/tessera-order/benchmark-data/comparison.json",
});

const data = shallowRef<ComparisonData | null>(null);
const error = shallowRef("");
onMounted(async () => {
  let response: Response;
  try {
    response = await fetch(props.source, { cache: "no-cache" });
  } catch (cause) {
    console.error("Algorithm comparison:", cause);
    error.value = "The comparison data could not be reached. Check the connection and reload.";
    return;
  }
  if (!response.ok) {
    error.value = `The comparison data could not be downloaded (HTTP ${response.status}).`;
    return;
  }
  try {
    data.value = comparisonSchema.parse(await response.json());
  } catch (cause) {
    console.error("Algorithm comparison:", cause);
    error.value = "The published comparison data has a format this page cannot read.";
  }
});

// Inline SVG follows the page's light or dark class; each chart needs its own element ids.
const id = useId();
const figures = computed(() => {
  const list = data.value
    ? props.view === "overview"
      ? data.value.overview
        ? [data.value.overview]
        : []
      : data.value.figures
    : [];
  return list.map((figure) => ({
    name: figure.name,
    svg: renderRows({ ...figure.chart, id: `${id}-${figure.name}` }),
    table: cells(figure.table),
  }));
});
// The figure's Markdown table as rows of plain-text cells, rendered by Vue as text.
function cells(markdown: string) {
  const rows = markdown
    .trim()
    .split("\n")
    .filter((line) => !/^\|\s*-/.test(line))
    .map((line) =>
      line
        .replace(/^\|/, "")
        .replace(/\|$/, "")
        .split("|")
        .map((cell) => cell.trim()),
    );
  return { head: rows[0] ?? [], body: rows.slice(1) };
}
const tessera = computed(() => {
  const run = data.value?.runs.find((entry) => entry.algorithm === "tessera");
  return run && {
    short: run.hash.slice(0, 7),
    link: `https://github.com/Yikai-Liao/tessera-order/commit/${run.hash}`,
  };
});
const summary = computed(() => {
  const days = (data.value?.runs ?? []).map((run) => run.time.slice(0, 10)).toSorted();
  const span = days[0] === days.at(-1) ? days[0] : `${days[0]} to ${days.at(-1)}`;
  return `Latest run of each sort on Bencher Intel v1 bare metal, ${span}${tessera.value ? "; Tessera at" : ""}`;
});
const unmeasured = computed(() =>
  (data.value?.runs ?? []).filter((run) => !run.memory).map((run) => run.label),
);
</script>

<template>
  <div class="algorithm-comparison">
    <p v-if="error" class="notice" role="alert">{{ error }}</p>
    <p v-else-if="!data" class="notice" role="status">Loading the latest measurements…</p>
    <p v-else-if="!figures.length" class="notice">No cross-algorithm figure is published yet.</p>
    <template v-else>
      <figure v-for="figure in figures" :key="figure.name" class="figure">
        <!-- The shared renderer escapes labels through DOM text nodes. -->
        <div v-html="figure.svg" />
        <details v-if="figure.table.body.length" class="data">
          <summary>Data</summary>
          <table>
            <thead>
              <tr>
                <th v-for="cell in figure.table.head" :key="cell">{{ cell }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="(row, index) in figure.table.body" :key="index">
                <td v-for="(cell, column) in row" :key="column">{{ cell }}</td>
              </tr>
            </tbody>
          </table>
        </details>
      </figure>
      <p class="provenance">
        {{ summary }}
        <a v-if="tessera" :href="tessera.link">{{ tessera.short }}</a
        >.<template v-if="unmeasured.length">
          Memory not yet recorded for {{ unmeasured.join(", ") }}.</template
        >
      </p>
      <p v-if="view === 'overview'" class="more">
        <a :href="withBase('/benchmarks/algorithms')">All comparison figures →</a>
      </p>
    </template>
  </div>
</template>

<style scoped>
.algorithm-comparison {
  max-width: 960px;
  margin: 24px 0;
}
.figure {
  margin: 0 0 24px;
}
.figure :deep(svg) {
  display: block;
  width: 100%;
  height: auto;
}
.data {
  margin-top: 8px;
  font-size: 13px;
}
.data summary {
  cursor: pointer;
  color: var(--vp-c-text-2);
}
.provenance {
  margin: 0;
  color: var(--vp-c-text-2);
  font-size: 14px;
}
.more {
  margin: 8px 0 0;
  font-size: 14px;
}
.notice {
  margin: 0;
  padding: 12px 16px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 8px;
  color: var(--vp-c-text-2);
  font-size: 14px;
}
</style>
