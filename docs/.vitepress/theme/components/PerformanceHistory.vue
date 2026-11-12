<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, useId, watch } from "vue";
import { renderHistory } from "../../../../bench/plot/history.ts";
import { indexSchema, PREVIEW_CASE, parseRuns, type TrackingRun } from "../../../../bench/tracking/result.ts";

const props = withDefaults(defineProps<{ source?: string; caseId?: string }>(), {
  source: "https://raw.githubusercontent.com/Yikai-Liao/tessera-sort/benchmark-data/main/",
  caseId: PREVIEW_CASE,
});
const index = ref<ReturnType<typeof indexSchema.parse> | null>(null);
const runs = ref<TrackingRun[]>([]);
const selected = ref("");
const range = ref("90");
const error = ref("");
const loading = ref(true);
const initial = new AbortController();
onUnmounted(() => initial.abort());
const since = computed(() => range.value === "all" ? "" : new Date(Date.now() - Number(range.value) * 86400000).toISOString());
async function download(file: string, signal: AbortSignal) {
  const response = await fetch(props.source + file, { signal, cache: file === "index.json" ? "no-cache" : "default" });
  if (!response.ok) throw new Error("Performance data unavailable (HTTP " + response.status + ")");
  return response.text();
}
onMounted(async () => {
  try {
    const snapshot = indexSchema.parse(JSON.parse(await download("index.json", initial.signal)));
    selected.value = snapshot.cases.find((entry) => entry.id === props.caseId)?.id ?? snapshot.cases[0]?.id ?? "";
    index.value = snapshot;
  } catch (cause) {
    if (!initial.signal.aborted) { error.value = String(cause); loading.value = false; }
  }
});
watch([index, range], async ([snapshot], _, cleanup) => {
  if (!snapshot) return;
  const controller = new AbortController();
  cleanup(() => controller.abort());
  loading.value = true;
  error.value = "";
  runs.value = [];
  try {
    const months = snapshot.months.filter(({ month }) => !since.value || month >= since.value.slice(0, 7));
    const batches = await Promise.all(months.map(async ({ file }) => parseRuns(await download(file, controller.signal))));
    if (!controller.signal.aborted) runs.value = batches.flat().sort((a, b) => a.time.localeCompare(b.time));
  } catch (cause) {
    if (!controller.signal.aborted) error.value = String(cause);
  } finally {
    if (!controller.signal.aborted) loading.value = false;
  }
});
const history = computed(() => runs.value.filter((run) => run.time >= since.value && run.values[selected.value]).map((run) => ({
  run, time: new Date(run.time), value: run.values[selected.value][0],
  lower: run.values[selected.value][1], upper: run.values[selected.value][2], series: run.protocol + "/" + run.spec,
  title: run.time + "\n" + run.hash.slice(0, 7) + "\n" + run.rustc,
  href: "https://github.com/Yikai-Liao/tessera-sort/commit/" + run.hash,
})));
const chartId = useId();
const chart = computed(() => renderHistory(history.value, chartId));
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
      <details v-if="history.length"><summary>Values and evidence ({{ history.length }} runs)</summary>
        <table><thead><tr><th>UTC date</th><th>Commit</th><th>Time (ns)</th><th>Criterion interval (ns)</th><th>Evidence</th></tr></thead>
          <tbody><tr v-for="point in history.slice().reverse()" :key="point.run.id">
            <td>{{ point.run.time }}</td><td><a :href="point.href">{{ point.run.hash.slice(0, 7) }}</a></td>
            <td>{{ point.value.toPrecision(5) }}</td><td>{{ point.lower.toPrecision(5) }}–{{ point.upper.toPrecision(5) }}</td>
            <td><a :href="point.run.workflow">Actions run</a></td>
          </tr></tbody>
        </table>
      </details>
    </template>
  </section>
</template>

<style scoped>
.controls { display: flex; flex-wrap: wrap; gap: 12px; margin: 20px 0; }
select { border: 1px solid var(--vp-c-divider); border-radius: 4px; padding: 6px; max-width: 100%; background: var(--vp-c-bg); }
.chart { margin: 20px 0; }
.chart :deep(svg) { display: block; width: 100%; height: auto; }
details { overflow-x: auto; }
</style>
