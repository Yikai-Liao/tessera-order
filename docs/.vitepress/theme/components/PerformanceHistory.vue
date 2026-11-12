<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { computed, onMounted, ref, useId, watch } from "vue";
import { renderHistory } from "../../../../bench/tracking/history.ts";
import { indexSchema, PREVIEW_CASE, parseRuns, type TrackingRun } from "../../../../bench/tracking/result.ts";

const props = withDefaults(defineProps<{ source?: string; caseId?: string }>(), {
  source: "https://raw.githubusercontent.com/Yikai-Liao/tessera-sort/benchmark-data/main/",
  caseId: PREVIEW_CASE,
});
const index = ref<ReturnType<typeof indexSchema.parse> | null>(null);
const runs = ref<TrackingRun[]>([]);
const selected = ref(props.caseId);
const range = ref("90");
const error = ref("");
const loading = ref(true);
const since = computed(() => range.value === "all" ? "" : new Date(Date.now() - Number(range.value) * 86400000).toISOString());
onMounted(() => watch([() => props.source, since], async ([source, start], _, cleanup) => {
  const controller = new AbortController();
  cleanup(() => controller.abort());
  loading.value = true;
  error.value = "";
  runs.value = [];
  async function download(file: string) {
    const response = await fetch(source + file, { signal: controller.signal, cache: "no-cache" });
    if (!response.ok) throw new Error("Performance data unavailable (HTTP " + response.status + ")");
    return response.text();
  }
  try {
    const snapshot = indexSchema.parse(JSON.parse(await download("index.json")));
    const months = snapshot.months.filter((month) => !start || month >= start.slice(0, 7));
    const batches = await Promise.all(months.map(async (month) => parseRuns(await download(month + ".jsonl"))));
    if (controller.signal.aborted) return;
    index.value = snapshot;
    if (!snapshot.cases.some((entry) => entry.id === selected.value)) selected.value = snapshot.cases[0]?.id ?? "";
    runs.value = batches.flat();
  } catch (cause) {
    if (!controller.signal.aborted) error.value = String(cause);
  } finally {
    if (!controller.signal.aborted) loading.value = false;
  }
}, { immediate: true }));
const history = computed(() => runs.value.filter((run) => run.time >= since.value && run.values[selected.value]));
const chartId = useId();
const chart = computed(() => renderHistory(history.value, selected.value, chartId));
</script>

<template>
  <section aria-label="Main performance history">
    <p v-if="error" role="alert">{{ error }}. Refresh to load the latest data index.</p>
    <p v-if="loading" role="status">Loading performance history…</p>
    <p v-else-if="index && !index.months.length">No main measurements have been published yet.</p>
    <template v-if="index?.cases.length">
      <div class="controls">
        <label>Case <select v-model="selected"><option v-for="entry in index.cases" :key="entry.id" :value="entry.id">{{ entry.label }}</option></select></label>
        <label>Window <select v-model="range"><option value="30">30 days</option><option value="90">90 days</option><option value="365">One year</option><option value="all">All history</option></select></label>
      </div>
      <p>Lower is faster. Compiler and dependency upgrades remain in the trend.</p>
      <p v-if="!loading && !error && !history.length">No observations in this window.</p>
      <!-- The shared renderer escapes labels through DOM text nodes. -->
      <div class="chart" v-html="chart" />
    </template>
  </section>
</template>

<style scoped>
.controls { display: flex; flex-wrap: wrap; gap: 12px; margin: 20px 0; }
select { border: 1px solid var(--vp-c-divider); border-radius: 4px; padding: 6px; max-width: 100%; background: var(--vp-c-bg); }
.chart { margin: 20px 0; }
.chart :deep(svg) { display: block; width: 100%; height: auto; }
</style>
