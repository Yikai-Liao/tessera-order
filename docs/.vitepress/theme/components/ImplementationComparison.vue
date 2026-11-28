<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { getScrollOffset, withBase } from "vitepress";
import { contentUpdatedCallbacks } from "vitepress/dist/client/app/utils.js";
import { computed, onMounted, onUnmounted, shallowRef, useId, watch } from "vue";
import { type CurveChart, renderCurves } from "../../../../bench/plot/curves.ts";
import { type RowChart, renderRows } from "../../../../bench/plot/rows.ts";
import { type ComparisonData, comparisonSchema } from "../../../../bench/tracking/result.ts";
import {
  COMPARISON_DATA_URL,
  type ComparisonFigure,
  type ComparisonView,
  comparisonHeadlines,
  comparisonSections,
  comparisonTable,
} from "../../comparisonMarkdown.ts";
import { data as snapshot } from "./comparisonSnapshot.data.ts";
import { useFigureSelection } from "./figureSelection.ts";
import { data as implementationChoices } from "./implementationChoices.data.ts";

// `headlines` renders compact proof cards linking to `/benchmarks/implementations#<op>`;
// `overview` shows the sort ranking figure; `all` shows every operation's figures.
const props = withDefaults(defineProps<{ view?: ComparisonView; source?: string }>(), {
  view: "all",
  source:
    (import.meta.env.DEV && import.meta.env.VITE_COMPARISON_SOURCE) || COMPARISON_DATA_URL,
});

const defaultSource =
  (import.meta.env.DEV && import.meta.env.VITE_COMPARISON_SOURCE) || COMPARISON_DATA_URL;
// Initialize from the build-time snapshot when using the default source so SSR emits real
// cards, SVGs, provenance, and tables; keep explicit local preview overrides authoritative.
const usingSnapshot = props.source === defaultSource;
const data = shallowRef<ComparisonData | null>(usingSnapshot ? snapshot.data : null);
const error = shallowRef("");
const staleNotice = shallowRef("");
const retrying = shallowRef(false);
let scrolledToHash = false;
function scrollToFragment() {
  if (!location.hash || scrolledToHash) return;
  let id: string;
  try {
    id = decodeURIComponent(location.hash.slice(1));
  } catch {
    return;
  }
  const target = document.getElementById(id);
  if (!target) return;
  scrolledToHash = true;
  // The router's own offset clears the navbar and, at medium widths, the outline bar below it.
  window.scrollTo(
    0,
    scrollY +
      target.getBoundingClientRect().top -
      getScrollOffset() +
      parseInt(getComputedStyle(target).paddingTop, 10),
  );
}
// When data arrives or gains newly published sections after mount, refresh the outline and
// resolve any URL fragment once its target heading renders.
watch(
  data,
  (current) => {
    if (props.view !== "all" || !current) return;
    for (const refresh of contentUpdatedCallbacks) refresh();
    scrollToFragment();
  },
  { flush: "post" },
);

// Each card names the operation, Tessera's standing and the two numbers behind it.
const cards = computed(() => (data.value ? comparisonHeadlines(data.value) : []));
let refreshController: AbortController | null = null;

async function refreshLive() {
  refreshController?.abort();
  const controller = new AbortController();
  refreshController = controller;
  retrying.value = true;
  try {
    let response: Response;
    try {
      response = await fetch(props.source, { cache: "no-cache", signal: controller.signal });
    } catch (cause) {
      if (controller.signal.aborted) return;
      console.error("Implementation comparison:", cause);
      if (data.value) {
        staleNotice.value =
          "Live refresh failed; showing the site's build-time snapshot. Newer measurements may be published.";
      } else {
        error.value = "The comparison data could not be reached. Check the connection and reload.";
      }
      return;
    }
    if (!response.ok) {
      if (data.value) {
        staleNotice.value = `Live refresh failed (HTTP ${response.status}); showing the site's build-time snapshot.`;
      } else {
        error.value = `The comparison data could not be downloaded (HTTP ${response.status}).`;
      }
      return;
    }
    try {
      const next = comparisonSchema.parse(await response.json());
      if (controller.signal.aborted) return;
      if (data.value && JSON.stringify(next) === JSON.stringify(data.value)) {
        error.value = "";
        staleNotice.value = "";
        return;
      }
      // Validate only the charts this view renders so unchanged or non-chart views do zero extra SVG work.
      for (const fig of comparisonSections(next, props.view).flatMap((section) => section.figures)) {
        if (fig.kind === "curves") renderCurves({ ...(fig.chart as CurveChart), id: "check" });
        else renderRows({ ...(fig.chart as RowChart), id: "check" });
      }
      error.value = "";
      staleNotice.value = "";
      data.value = next;
    } catch (cause) {
      if (controller.signal.aborted) return;
      console.error("Implementation comparison:", cause);
      if (data.value) {
        staleNotice.value =
          "Live comparison data could not be read; showing the site's build-time snapshot.";
      } else {
        error.value = "The published comparison data has a format this page cannot read.";
      }
    }
  } finally {
    if (!controller.signal.aborted) retrying.value = false;
  }
}

onMounted(async () => {
  window.addEventListener("keydown", clearOnEscape);
  if (props.view === "all" && data.value) {
    for (const refresh of contentUpdatedCallbacks) refresh();
    scrollToFragment();
  }
  if (usingSnapshot && snapshot.overridden && data.value) return;
  await refreshLive();
});
onUnmounted(() => {
  window.removeEventListener("keydown", clearOnEscape);
  refreshController?.abort();
});

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
    table: comparisonTable(figure.table),
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
  <div ref="root" class="implementation-comparison" :class="{ 'is-strip': view === 'headlines' }">
    <h2 v-if="view === 'all' && cards.length" id="at-a-glance" class="operation-title" tabindex="-1">At a glance</h2>
    <div v-if="view !== 'overview' && cards.length" class="cards">
      <a v-for="card in cards" :key="card.id" class="card"
        :href="view === 'headlines' ? withBase(`/benchmarks/implementations#${card.id}`) : `#${card.id}`">
        <span class="card-operation">{{ card.headline.operation }}</span>
        <span class="card-claim">{{ card.claim }}</span>
        <span class="card-stats">
          <span class="card-stat">
            <strong>{{ card.speedText }}</strong>
            <span>{{ view === 'headlines' ? 'vs std' : 'speed vs the standard library' }}</span>
          </span>
          <span v-if="card.memoryText !== null" class="card-stat">
            <strong>{{ card.memoryText }}</strong>
            <span>{{ view === 'headlines' ? 'extra' : 'extra memory' }}<template v-if="card.referenceMemoryText !== null">
              {{ view === 'headlines' ? ' (vs ' : ', vs ' }}<span class="nowrap">{{ card.referenceMemoryText }}</span>{{ view === 'headlines' ? ')' : '' }}
            </template></span>
          </span>
        </span>
      </a>
    </div>
    <p v-if="view === 'headlines'" class="proof-foot">
      <span v-if="staleNotice" role="status">
        {{ staleNotice }}
        <button type="button" class="stale-retry" :disabled="retrying" @click="refreshLive">
          {{ retrying ? "Retrying…" : "Retry" }}
        </button>
        · <a :href="props.source" target="_blank" rel="noopener">Live JSON</a>
      </span>
      <span v-else>{{ cards.length ? 'Measured on Bencher Intel v1 bare metal against Rust & C++ sorts.' : 'Live benchmark measurements are published on the comparison page.' }}</span>
      <a :href="withBase('/benchmarks/implementations')">Full comparison →</a>
    </p>
    <p v-else-if="error" class="notice" role="alert">{{ error }}</p>
    <p v-else-if="!data" class="notice" role="status">Loading the latest measurements…</p>
    <p v-else-if="!sections.length" class="notice">
      No cross-implementation figure is published yet.
    </p>
    <template v-else>
      <p v-if="staleNotice" class="notice stale-notice" role="status">
        {{ staleNotice }}
        <button type="button" class="stale-retry" :disabled="retrying" @click="refreshLive">
          {{ retrying ? "Retrying…" : "Retry" }}
        </button>
        · <a :href="props.source" target="_blank" rel="noopener">Live comparison.json</a>
      </p>
      <template v-if="view === 'all'">
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
.implementation-comparison.is-strip {
  max-width: 100%;
  margin: 8px 0 32px;
}
.proof-foot {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  gap: 6px 16px;
  margin: 10px 2px 0;
  color: var(--vp-c-text-2);
  font-size: 13px;
}
.proof-foot a {
  font-weight: 500;
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
.cards {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 16px;
  margin: 16px 0 24px;
}
@media (max-width: 639px) {
  .cards {
    grid-template-columns: minmax(0, 1fr);
  }
}
.card {
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
.card:hover {
  border-color: var(--vp-c-brand-1);
}
.card-operation {
  font-size: 20px;
  font-weight: 600;
  line-height: 1.3;
}
.card-claim {
  color: var(--vp-c-brand-1);
  font-size: 14px;
  font-weight: 600;
}
.card-stats {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 16px;
  margin: 12px 0 0;
}
.card-stat {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.card-stat strong {
  color: var(--vp-c-brand-1);
  font-size: 30px;
  font-weight: 600;
  line-height: 1.15;
  font-variant-numeric: tabular-nums;
}
.card-stat span {
  color: var(--vp-c-text-2);
  font-size: 13px;
  line-height: 1.45;
}
.card-stat .nowrap {
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
.stale-notice {
  margin-bottom: 16px;
}
.stale-retry {
  margin-left: 6px;
  padding: 1px 8px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 6px;
  background: var(--vp-c-bg);
  color: var(--vp-c-text-1);
  font-size: 12px;
  cursor: pointer;
}
.stale-retry:hover:not(:disabled) {
  border-color: var(--vp-c-brand-1);
  color: var(--vp-c-brand-1);
}
.stale-retry:disabled {
  opacity: 0.6;
  cursor: default;
}
.is-strip .cards { grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 12px; margin: 0; }
.is-strip .card { gap: 4px; padding: 16px 18px; transition: border-color 0.2s ease, transform 0.15s ease; }
.is-strip .card:hover { transform: translateY(-1px); }
.is-strip .card-operation { font-size: 14px; }
.is-strip .card-claim { font-size: 12px; line-height: 1.35; }
.is-strip .card-stats { display: flex; flex-wrap: wrap; gap: 10px 16px; margin-top: 8px; }
.is-strip .card-stat { gap: 1px; }
.is-strip .card-stat strong { color: var(--vp-c-text-1); font-size: 22px; font-weight: 700; }
.is-strip .card-stat span { font-size: 12px; line-height: 1.35; }
@media (max-width: 959px) { .is-strip .cards { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
@media (max-width: 559px) { .is-strip .cards { grid-template-columns: minmax(0, 1fr); } }
</style>
