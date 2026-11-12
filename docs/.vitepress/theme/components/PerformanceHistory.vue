<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, useId, useTemplateRef, watch } from "vue";
import { formatChange } from "../../../../bench/plot/annotations.ts";
import {
  formatTime,
  latestValues,
  renderCaseHistory,
  renderMainTrend,
} from "../../../../bench/tracking/history.ts";
import { indexSchema, parseRuns, type TrackingRun } from "../../../../bench/tracking/result.ts";

const props = withDefaults(defineProps<{ source?: string; caseId?: string }>(), {
  source: "https://raw.githubusercontent.com/Yikai-Liao/tessera-sort/benchmark-data/main/",
  caseId: "rust/u64/random/65536/s1/api",
});
const WINDOWS = [
  ["30", "30 days"],
  ["90", "90 days"],
  ["365", "1 year"],
  ["all", "All"],
] as const;
const index = ref<ReturnType<typeof indexSchema.parse> | null>(null);
const loaded = ref<TrackingRun[]>([]);
const range = ref<(typeof WINDOWS)[number][0]>("90");
const error = ref("");
const loading = ref(true);
const since = computed(() =>
  range.value === "all" ? "" : new Date(Date.now() - Number(range.value) * 86400000).toISOString(),
);

/** A failure readers can act on; the underlying cause goes to the console only. */
class DataError extends Error {}
async function download(source: string, file: string, signal: AbortSignal) {
  let response: Response;
  try {
    response = await fetch(source + file, { signal, cache: "no-cache" });
  } catch (cause) {
    if (signal.aborted) throw cause;
    throw new DataError(
      "The performance data could not be reached. Check the connection and reload the page.",
      { cause },
    );
  }
  if (!response.ok)
    throw new DataError(`The performance data could not be downloaded (HTTP ${response.status}). Reload the page to try again.`);
  return response.text();
}
function readable<T>(read: () => T) {
  try {
    return read();
  } catch (cause) {
    throw new DataError("The published performance data has a format this page cannot read.", { cause });
  }
}
onMounted(() =>
  watch(
    [() => props.source, since],
    async ([source, start], _, cleanup) => {
      const controller = new AbortController();
      cleanup(() => controller.abort());
      loading.value = true;
      error.value = "";
      try {
        const text = await download(source, "index.json", controller.signal);
        const snapshot = readable(() => indexSchema.parse(JSON.parse(text)));
        const months = snapshot.months.filter((month) => !start || month >= start.slice(0, 7));
        const batches = await Promise.all(
          months.map(async (month) => {
            const journal = await download(source, month + ".jsonl", controller.signal);
            return readable(() => parseRuns(journal));
          }),
        );
        if (controller.signal.aborted) return;
        index.value = snapshot;
        loaded.value = batches.flat();
      } catch (cause) {
        if (controller.signal.aborted) return;
        console.error("Performance history:", cause);
        error.value =
          cause instanceof DataError ? cause.message : "The performance data could not be displayed.";
        // Never show earlier data under the newly selected window.
        index.value = null;
        loaded.value = [];
      } finally {
        if (!controller.signal.aborted) loading.value = false;
      }
    },
    { immediate: true },
  ),
);
const runs = computed(() => loaded.value.filter((run) => run.time >= since.value));
const cases = computed(() => index.value?.cases ?? []);

// The URL hash owns the selected case, so the browser's history and shared links both work.
// Row links are ordinary anchors: the site router and plain pages both emit hashchange.
const link = (id: string) => "#case=" + encodeURIComponent(id);
function hashCase() {
  const match = /^#case=(.+)$/.exec(location.hash);
  try {
    return match ? decodeURIComponent(match[1]) : "";
  } catch {
    return "";
  }
}
const requested = ref(props.caseId);
const selected = computed(
  () => cases.value.find((entry) => entry.id === requested.value) ?? cases.value[0],
);
const detail = useTemplateRef<HTMLElement>("detail");
const overview = useTemplateRef<HTMLElement>("overview");
function follow() {
  const id = hashCase();
  requested.value = id || props.caseId;
  if (id) nextTick(() => detail.value?.scrollIntoView({ behavior: "smooth", block: "start" }));
}
onMounted(() => {
  requested.value = hashCase() || props.caseId;
  window.addEventListener("hashchange", follow);
});
onUnmounted(() => window.removeEventListener("hashchange", follow));
// A shared link scrolls to its case once, after the first data arrives.
const stopInitialScroll = watch(loading, (busy) => {
  if (busy) return;
  stopInitialScroll();
  follow();
});

const trendId = useId();
const historyId = useId();
const trend = computed(() => renderMainTrend(runs.value, cases.value, trendId, link));
const history = computed(() =>
  selected.value ? renderCaseHistory(runs.value, selected.value, historyId) : "",
);
const table = computed(() => latestValues(runs.value, cases.value));
// After each render, mark the selected row. The figure is an image to assistive
// technology, so its row links stay out of the focus order; the table's links
// offer the same selection by keyboard.
watch(
  [trend, selected],
  async () => {
    await nextTick();
    for (const anchor of overview.value?.querySelectorAll(".trend-row-link a") ?? []) {
      anchor.classList.toggle("selected", anchor.getAttribute("href") === link(selected.value?.id ?? ""));
      anchor.setAttribute("tabindex", "-1");
      anchor.setAttribute("aria-hidden", "true");
    }
  },
  { flush: "post" },
);
</script>

<template>
  <section class="history" aria-label="Main performance history">
    <div class="toolbar">
      <fieldset class="windows" aria-label="Time window">
        <button
          v-for="[value, name] in WINDOWS"
          :key="value"
          type="button"
          :aria-pressed="range === value"
          @click="range = value"
        >
          {{ name }}
        </button>
      </fieldset>
      <span v-if="loading" role="status">Loading performance history…</span>
    </div>
    <p v-if="error" class="notice" role="alert">{{ error }}</p>
    <p v-else-if="index && !index.months.length" class="notice">No main measurements have been published yet.</p>
    <p v-else-if="!loading && index && !runs.length" class="notice">No observations in this window.</p>
    <!-- The shared renderer escapes labels through DOM text nodes. -->
    <div v-if="trend" ref="overview" class="chart" v-html="trend" />
    <div v-if="history" ref="detail" class="chart detail" v-html="history" />
    <details v-if="table.length">
      <summary>Latest values of every case</summary>
      <table>
        <thead>
          <tr>
            <th>Case</th>
            <th>Mode</th>
            <th class="number">Latest</th>
            <th class="number">Interval</th>
            <th class="number">Change</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in table" :key="row.entry.id">
            <td><a :href="link(row.entry.id)">{{ row.entry.label }}</a></td>
            <td>{{ row.entry.group }}</td>
            <td class="number">
              {{ formatTime(row.value) }}<span v-if="!row.current" class="stale"> ({{ row.time.slice(0, 10) }})</span>
            </td>
            <td class="number">{{ formatTime(row.lower) }} – {{ formatTime(row.upper) }}</td>
            <td class="number">{{ formatChange(row.change) }}</td>
          </tr>
        </tbody>
      </table>
    </details>
  </section>
</template>

<style scoped>
.toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 16px;
  margin: 20px 0 16px;
  color: var(--vp-c-text-2);
  font-size: 13px;
}
.windows {
  display: inline-flex;
  margin: 0;
  padding: 0;
  min-width: 0;
  border: 1px solid var(--vp-c-divider);
  border-radius: 8px;
  overflow: hidden;
}
.windows button {
  padding: 4px 14px;
  font-size: 13px;
  color: var(--vp-c-text-2);
}
.windows button + button {
  border-left: 1px solid var(--vp-c-divider);
}
.windows button[aria-pressed="true"] {
  background: var(--vp-c-default-soft);
  color: var(--vp-c-text-1);
  font-weight: 600;
}
.chart {
  margin: 0 0 20px;
}
/* Natural size from the SVG's own width; shrink only when the column is narrower. */
.chart :deep(svg) {
  display: block;
  max-width: 100%;
  height: auto;
}
.chart :deep(.trend-row-link a:hover rect),
.chart :deep(.trend-row-link a:focus-visible rect),
.chart :deep(.trend-row-link a.selected rect) {
  fill: var(--vp-c-default-soft);
}
.chart :deep(.trend-row-link a:focus-visible rect) {
  stroke: var(--vp-c-brand-1);
}
.detail {
  scroll-margin-top: calc(var(--vp-nav-height) + 16px);
}
.notice {
  margin: 0 0 20px;
  padding: 12px 16px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 8px;
  color: var(--vp-c-text-2);
  font-size: 14px;
}
.stale {
  color: var(--vp-c-text-3);
}
details table {
  font-size: 13px;
}
.number {
  text-align: right;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}
</style>
