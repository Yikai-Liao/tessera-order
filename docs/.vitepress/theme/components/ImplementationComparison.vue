<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { getScrollOffset, withBase } from "vitepress";
import { contentUpdatedCallbacks } from "vitepress/dist/client/app/utils.js";
import { computed, onMounted, onUnmounted, shallowRef, useId, watch } from "vue";
import { type CurveChart, renderCurves } from "../../../../bench/plot/curves.ts";
import { type RowChart, renderRows } from "../../../../bench/plot/rows.ts";
import type { ComparisonData } from "../../../../bench/tracking/result.ts";
import {
  COMPARISON_DATA_URL,
  type ComparisonFigure,
  comparisonHeadlines,
  comparisonSections,
  parseComparisonData,
  parseMarkdownTable,
} from "../../comparisonMarkdown.ts";
import { data as snapshot } from "./comparisonSnapshot.data.ts";
import { useFigureSelection } from "./figureSelection.ts";
import { data as implementationChoices } from "./implementationChoices.data.ts";

// `overview` shows the headline ranking with a link to the full page; `all` shows every figure.
const props = withDefaults(defineProps<{ view?: "overview" | "all"; source?: string }>(), {
  view: "all",
  source:
    (import.meta.env.DEV && import.meta.env.VITE_COMPARISON_SOURCE) || COMPARISON_DATA_URL,
});

const defaultSource =
  (import.meta.env.DEV && import.meta.env.VITE_COMPARISON_SOURCE) || COMPARISON_DATA_URL;
// Initialize from the build-time snapshot when using the default source so SSR emits real
// cards, SVGs, provenance, and tables; `onMounted` refreshes live without erasing SSR data.
const data = shallowRef<ComparisonData | null>(props.source === defaultSource ? snapshot : null);
const error = shallowRef("");
// When data arrives after mount (offline build or custom source), refresh the outline and
// resolve any URL fragment once headings render. `contentUpdatedCallbacks` is a private
// VitePress 1.6 export (declared in env.d.ts); after an upgrade, verify both outlines list these headings.
watch(
  data,
  (current, previous) => {
    if (props.view !== "all" || !current) return;
    for (const refresh of contentUpdatedCallbacks) refresh();
    if (previous) return;
    const target = location.hash && document.getElementById(decodeURIComponent(location.hash.slice(1)));
    // The router's own offset clears the navbar and, at medium widths, the outline bar below it.
    if (target)
      window.scrollTo(
        0,
        scrollY +
          target.getBoundingClientRect().top -
          getScrollOffset() +
          parseInt(getComputedStyle(target).paddingTop, 10),
      );
  },
  { flush: "post" },
);

// Each card names the operation, Tessera's standing and the two numbers behind it.
const cards = computed(() => (data.value ? comparisonHeadlines(data.value) : []));

onMounted(async () => {
  window.addEventListener("keydown", clearOnEscape);
  if (props.view === "all" && data.value) {
    for (const refresh of contentUpdatedCallbacks) refresh();
  }
  let response: Response;
  try {
    response = await fetch(props.source, { cache: "no-cache" });
  } catch (cause) {
    console.error("Implementation comparison:", cause);
    if (!data.value)
      error.value = "The comparison data could not be reached. Check the connection and reload.";
    return;
  }
  if (!response.ok) {
    if (!data.value)
      error.value = `The comparison data could not be downloaded (HTTP ${response.status}).`;
    return;
  }
  try {
    const next = parseComparisonData(await response.json());
    error.value = "";
    if (!data.value || JSON.stringify(next) !== JSON.stringify(data.value)) data.value = next;
  } catch (cause) {
    console.error("Implementation comparison:", cause);
    if (!data.value)
      error.value = "The published comparison data has a format this page cannot read.";
  }
});
onUnmounted(() => window.removeEventListener("keydown", clearOnEscape));

// Inline SVG follows the page's light or dark class; each chart needs its own element ids.
const id = useId();
const rendered = (operation: string, figure: ComparisonFigure) => {
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
    selection: figure.selection,
    table: parseMarkdownTable(figure.table),
  };
};
// The overview is the sort ranking; the full page shows each operation's section in order.
const sections = computed(() =>
  data.value
    ? comparisonSections(data.value, props.view).map((section) => ({
        ...section,
        figures: section.figures.map((figure) => rendered(section.id, figure)),
      }))
    : [],
);
const root = shallowRef<HTMLElement | null>(null);
const { pinned, bar, choices, preview, pin, clear } = useFigureSelection(
  () => root.value,
  () => sections.value.flatMap((section) => section.figures),
  implementationChoices,
);
function clearOnEscape(event: KeyboardEvent) {
  if (event.key === "Escape") clear();
}
</script>

<template>
  <div ref="root" class="implementation-comparison">
    <p v-if="error" class="notice" role="alert">{{ error }}</p>
    <p v-else-if="!data" class="notice" role="status">Loading the latest measurements…</p>
    <p v-else-if="!sections.length" class="notice">
      No cross-implementation figure is published yet.
    </p>
    <template v-else>
      <template v-if="view === 'all'">
        <h2 v-if="cards.length" id="at-a-glance" class="operation-title" tabindex="-1">At a glance</h2>
        <div v-if="cards.length" class="headlines">
          <a v-for="card in cards" :key="card.id" class="headline" :href="`#${card.id}`">
            <span class="headline-operation">{{ card.headline.operation }}</span>
            <span class="headline-claim">{{ card.claim }}</span>
            <span class="headline-stats">
              <span class="headline-stat">
                <strong>{{ card.speedText }}</strong>
                <span>speed vs the standard library</span>
              </span>
              <span v-if="card.memoryText !== null" class="headline-stat">
                <strong>{{ card.memoryText }}</strong>
                <span>extra memory<template v-if="card.referenceMemoryText !== null"
                    >, vs
                    <span class="nowrap">{{ card.referenceMemoryText }}</span></template
                  ></span>
              </span>
            </span>
          </a>
        </div>
        <p class="lead">
          <strong>How to read the figures.</strong> In a speed figure each dot is one implementation,
          and further right is faster. Speed is relative to the reference the figure names, so 2× takes
          half the time. Tessera is the blue dot. Every figure lists its values in a table below it.
        </p>
      </template>
      <div v-if="view === 'all' && choices.length" class="highlight-control">
        <label :for="`${id}-highlight`">Highlight</label>
        <select :id="`${id}-highlight`" v-model="pinned">
          <option v-for="choice in choices" :key="choice.id" :value="choice.id">
            {{ choice.label }}
          </option>
        </select>
        <span>Hover or click a mark to highlight its implementation in every figure; the Speed and Rank columns then show its values. Esc returns to Tessera.</span>
      </div>
      <section v-for="section in sections" :key="section.id" class="operation">
        <template v-if="view === 'all'">
          <h2 :id="section.id" class="operation-title" tabindex="-1">{{ section.label }}</h2>
          <p v-if="section.provenance" class="provenance">
            {{ section.provenance.text }}
            <a v-if="section.provenance.tessera" :href="section.provenance.tessera.link">{{
              section.provenance.tessera.short
            }}</a
            >.<template v-if="section.provenance.unmeasured.length">
              Memory not yet recorded for
              {{ section.provenance.unmeasured.join(", ") }}.</template
            >
          </p>
        </template>
        <section v-for="figure in section.figures" :key="figure.anchor" class="figure">
          <!-- The full page sections each figure under the subject it compares. -->
          <h3 v-if="view === 'all' && figure.subject" :id="figure.anchor" class="subject" tabindex="-1">
            {{ figure.subject }}
          </h3>
          <p v-if="figure.summary" class="lead">{{ figure.summary }}</p>
          <!-- The shared renderer escapes labels through DOM text nodes. -->
          <!-- biome-ignore lint/a11y/noStaticElementInteractions lint/a11y/useKeyWithClickEvents: The SVG delegates pointer selection without making individual marks keyboard stops. Escape and the clear button reset it. -->
          <div
            :data-comparison-figure="figure.anchor"
            @pointerover="preview($event.target)"
            @pointerout="preview($event.relatedTarget)"
            @click="pin"
            v-html="figure.svg"
          />
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
          <p v-if="section.provenance" class="provenance">
            {{ section.provenance.text }}
            <a v-if="section.provenance.tessera" :href="section.provenance.tessera.link">{{
              section.provenance.tessera.short
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
    <Transition name="selection-bar">
      <div v-if="bar" class="selection-bar" role="status">
        <span class="selection-swatch" :style="{ background: bar.ink }" />
        <span class="selection-text">
          <strong>{{ bar.label }}</strong>
          <span v-if="bar.pinnedLabel" class="selection-detail">· pinned: {{ bar.pinnedLabel }}</span>
        </span>
        <button type="button" class="selection-clear" @click="clear">
          Back to Tessera <kbd>Esc</kbd>
        </button>
      </div>
    </Transition>
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
.headlines {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 16px;
  margin: 16px 0 24px;
}
@media (max-width: 639px) {
  .headlines {
    grid-template-columns: minmax(0, 1fr);
  }
}
.headline {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 20px 22px;
  border: 1px solid var(--tsr-line);
  border-radius: 12px;
  background: var(--tsr-panel);
  color: var(--vp-c-text-1);
  text-decoration: none;
  overflow-wrap: anywhere;
  transition: border-color 0.2s;
}
.headline:hover {
  border-color: var(--vp-c-brand-1);
}
.headline-operation {
  font-size: 20px;
  font-weight: 600;
  line-height: 1.3;
}
.headline-claim {
  color: var(--vp-c-brand-1);
  font-size: 14px;
  font-weight: 600;
}
.headline-stats {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 16px;
  margin: 12px 0 0;
}
.headline-stat {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.headline-stat strong {
  color: var(--vp-c-brand-1);
  font-size: 30px;
  font-weight: 600;
  line-height: 1.15;
  font-variant-numeric: tabular-nums;
}
.headline-stat span {
  color: var(--vp-c-text-2);
  font-size: 13px;
  line-height: 1.45;
}
.headline-stat .nowrap {
  white-space: nowrap;
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
.highlight-control {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px 12px;
  margin: 16px 0 8px;
  padding: 10px 14px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 8px;
  background: var(--vp-c-bg-soft);
}
.highlight-control label {
  font-size: 14px;
  font-weight: 600;
}
.highlight-control select {
  border: 1px solid var(--vp-c-divider);
  border-radius: 6px;
  padding: 4px 28px 4px 10px;
  background: var(--vp-c-bg)
    url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='6'%3E%3Cpath d='M1 1l4 4 4-4' fill='none' stroke='%23888' stroke-width='1.5'/%3E%3C/svg%3E")
    no-repeat right 10px center;
  color: var(--vp-c-text-1);
  font-size: 14px;
  appearance: none;
  cursor: pointer;
}
.highlight-control select:focus-visible {
  outline: 2px solid var(--vp-c-brand-1);
  outline-offset: 1px;
}
.highlight-control span {
  flex: 1 1 18em;
  color: var(--vp-c-text-2);
  font-size: 13px;
  line-height: 1.5;
}
.figure :deep(svg) {
  display: block;
  width: 100%;
  height: auto;
}
/* Selecting an implementation dims the rest, outlines its marks and colours the columns. */
.figure :deep([data-implementation]) {
  transition: opacity 0.15s ease;
}
.figure :deep(.dimmed) {
  opacity: 0.15;
}
.figure :deep([aria-label="dot"] .selected) {
  stroke: var(--ink);
  stroke-width: 1.5px;
  stroke-opacity: 1;
  fill-opacity: 1;
}
.figure :deep([aria-label="line"] .selected) {
  stroke-width: 3px;
  stroke-opacity: 1;
}
.figure :deep([data-implementation]) {
  cursor: pointer;
}
[data-selecting] :deep([data-comparison-cell]),
[data-selecting] :deep([data-comparison-heading]) {
  fill: var(--selection-ink);
  transition: fill 0.15s ease;
}
.figure :deep(.selection-description) {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}
.selection-bar {
  position: fixed;
  left: 50%;
  bottom: 20px;
  z-index: 30;
  transform: translateX(-50%);
  display: flex;
  align-items: center;
  gap: 12px;
  width: max-content;
  max-width: calc(100vw - 32px);
  padding: 8px 8px 8px 16px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 999px;
  background: var(--vp-c-bg-elv);
  box-shadow: var(--vp-shadow-3);
  font-size: 14px;
  line-height: 20px;
}
.selection-swatch {
  flex: none;
  width: 10px;
  height: 10px;
  border-radius: 50%;
}
.selection-text {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.selection-detail {
  margin-left: 6px;
  color: var(--vp-c-text-2);
}
.selection-clear {
  flex: none;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 4px 6px 4px 12px;
  border-radius: 999px;
  background: var(--vp-c-default-soft);
  color: var(--vp-c-text-1);
  font-weight: 500;
}
.selection-clear:hover {
  background: var(--vp-c-default-2);
}
.selection-clear kbd {
  padding: 0 6px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 4px;
  font-family: inherit;
  font-size: 11px;
  color: var(--vp-c-text-2);
}
.selection-bar-enter-active,
.selection-bar-leave-active {
  transition:
    opacity 0.15s ease,
    transform 0.15s ease;
}
.selection-bar-enter-from,
.selection-bar-leave-to {
  opacity: 0;
  transform: translate(-50%, 8px);
}
@media (max-width: 640px) {
  .selection-clear kbd {
    display: none;
  }
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
