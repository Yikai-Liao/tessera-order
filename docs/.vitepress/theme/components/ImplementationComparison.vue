<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { withBase } from "vitepress";
import { computed, onMounted, shallowRef, useId } from "vue";
import { type CurveChart, renderCurves } from "../../../../bench/plot/curves.ts";
import { type RowChart, renderRows } from "../../../../bench/plot/rows.ts";
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
    console.error("Implementation comparison:", cause);
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
    console.error("Implementation comparison:", cause);
    error.value = "The published comparison data has a format this page cannot read.";
  }
});

// Inline SVG follows the page's light or dark class; each chart needs its own element ids.
const id = useId();
type Figure = ComparisonData["operations"][number]["figures"][number];
const rendered = (operation: string, figure: Figure) => {
  const key = `${id}-${operation}-${figure.name}`;
  return {
    // Section anchors keep figure names unique across operations.
    anchor: `${operation}-${figure.name}`,
    subject: figure.subject,
    summary: figure.summary,
    svg:
      figure.kind === "curves"
        ? renderCurves({ ...(figure.chart as CurveChart), id: key })
        : renderRows({ ...(figure.chart as RowChart), id: key }),
    table: cells(figure.table),
  };
};
// The overview is the sort ranking; the full page shows each operation's section in order.
const sections = computed(() => {
  const value = data.value;
  if (!value) return [];
  if (props.view === "overview") {
    const sort = value.operations.find((operation) => operation.id === "sort");
    return value.overview && sort
      ? [
          {
            id: sort.id,
            label: sort.label,
            provenance: provenance(sort.runs),
            figures: [rendered(sort.id, value.overview)],
          },
        ]
      : [];
  }
  return value.operations.map((operation) => ({
    id: operation.id,
    label: operation.label,
    provenance: provenance(operation.runs),
    figures: operation.figures.map((figure) => rendered(operation.id, figure)),
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
type Run = ComparisonData["operations"][number]["runs"][number];
// Which runs a section's figures come from: the dates, Tessera's commit and sorts without memory.
function provenance(runs: Run[]) {
  const days = runs.map((run) => run.time.slice(0, 10)).toSorted();
  const span = days[0] === days.at(-1) ? days[0] : `${days[0]} to ${days.at(-1)}`;
  const tessera = runs.find((run) => run.implementation === "tessera");
  return {
    text: `Latest run of each implementation on Bencher Intel v1 bare metal, ${span}${tessera ? "; Tessera at" : ""}`,
    tessera: tessera && {
      short: tessera.hash.slice(0, 7),
      link: `https://github.com/Yikai-Liao/tessera-order/commit/${tessera.hash}`,
    },
    unmeasured: runs.filter((run) => !run.memory).map((run) => run.label),
  };
}
</script>

<template>
  <div class="implementation-comparison">
    <p v-if="error" class="notice" role="alert">{{ error }}</p>
    <p v-else-if="!data" class="notice" role="status">Loading the latest measurements…</p>
    <p v-else-if="!sections.length" class="notice">
      No cross-implementation figure is published yet.
    </p>
    <template v-else>
      <section v-for="section in sections" :key="section.id" class="operation">
        <template v-if="view === 'all'">
          <h2 :id="section.id" class="operation-title">{{ section.label }}</h2>
          <p class="provenance">
            {{ section.provenance.text }}
            <a v-if="section.provenance.tessera" :href="section.provenance.tessera!.link">{{
              section.provenance.tessera!.short
            }}</a
            >.<template v-if="section.provenance.unmeasured.length">
              Memory not yet recorded for
              {{ section.provenance.unmeasured.join(", ") }}.</template
            >
          </p>
        </template>
        <section v-for="figure in section.figures" :key="figure.anchor" class="figure">
          <!-- The full page sections each figure under the subject it compares. -->
          <h3 v-if="view === 'all' && figure.subject" :id="figure.anchor" class="subject">
            {{ figure.subject }}
          </h3>
          <p v-if="figure.summary" class="lead">{{ figure.summary }}</p>
          <!-- The shared renderer escapes labels through DOM text nodes. -->
          <div v-html="figure.svg" />
          <details v-if="figure.table.body.length" class="data">
            <summary>Values as a table</summary>
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
        </section>
        <template v-if="view === 'overview'">
          <p class="provenance">
            {{ section.provenance.text }}
            <a v-if="section.provenance.tessera" :href="section.provenance.tessera!.link">{{
              section.provenance.tessera!.short
            }}</a
            >.<template v-if="section.provenance.unmeasured.length">
              Memory not yet recorded for
              {{ section.provenance.unmeasured.join(", ") }}.</template
            >
          </p>
          <p class="more">
            <a :href="withBase('/benchmarks/implementations')">All comparison figures →</a>
          </p>
        </template>
      </section>
    </template>
  </div>
</template>

<style scoped>
.implementation-comparison {
  max-width: 960px;
  margin: 24px 0;
}
.figure {
  margin: 0 0 40px;
}
.operation-title {
  margin: 48px 0 8px;
  font-size: 26px;
  line-height: 1.3;
  font-weight: 600;
}
.subject {
  margin: 48px 0 8px;
  padding-top: 24px;
  border-top: 1px solid var(--vp-c-divider);
  font-size: 22px;
  line-height: 1.3;
  font-weight: 600;
}
.lead {
  margin: 0 0 16px;
  color: var(--vp-c-text-2);
  line-height: 1.6;
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
