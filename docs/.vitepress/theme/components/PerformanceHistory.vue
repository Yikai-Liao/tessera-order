<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { useData } from "vitepress";
import {
  computed,
  nextTick,
  onMounted,
  onUnmounted,
  ref,
  shallowRef,
  useId,
  useTemplateRef,
  watch,
} from "vue";
import { formatChange } from "../../../../bench/plot/annotations.ts";
import { theme } from "../../../../bench/plot/theme.ts";
import {
  type ComparedCase,
  type ComparisonAxis,
  comparableRuns,
  formatTime,
  formatTimestamp,
  type HistoryAxis,
  latestValues,
  renderComparison,
  renderMainTrend,
} from "../../../../bench/tracking/history.ts";
import {
  type Facet,
  indexSchema,
  parseRuns,
  type TrackingRun,
} from "../../../../bench/tracking/result.ts";

const props = withDefaults(defineProps<{ source?: string; caseId?: string }>(), {
  source: "https://raw.githubusercontent.com/Yikai-Liao/tessera-sort/benchmark-data/tessera/",
  caseId: "rust/u64/random/65536/s1/api",
});
type Color = ComparedCase["color"];
const DAY_MS = 86400000;
const PRESETS = [
  [30, "30 days"],
  [90, "90 days"],
  [365, "1 year"],
  [0, "All"],
] as const;
const DEFAULT_DAYS = 90;
const { isDark } = useData();

// The URL hash owns the compared cases and the time range, so a view can be
// shared. Bounds are ISO dates or times and an empty bound is open; a date
// bound covers its whole UTC day, so a range typed as dates includes both days.
const from = ref("");
const to = ref("");
const selection = ref<{ id: string; color: Color }[]>([]);
const daysAgo = (days: number) => new Date(Date.now() - days * DAY_MS).toISOString().slice(0, 10);
const lowerBound = (bound: string) =>
  bound ? Date.parse(bound.length === 10 ? `${bound}T00:00:00Z` : bound) : -Infinity;
const upperBound = (bound: string) =>
  bound.length === 10
    ? Date.parse(`${bound}T00:00:00Z`) + DAY_MS - 1
    : bound
      ? Date.parse(bound)
      : Infinity;
// Independent filters over the index's facets, each named for its URL parameter.
const FILTERS: { facet: Facet; key: string; name: string }[] = [
  { facet: "type", key: "type", name: "Type" },
  { facet: "input", key: "distribution", name: "Distribution" },
  { facet: "length", key: "length", name: "Length" },
  { facet: "group", key: "mode", name: "Mode" },
];
const filters = ref<Record<Facet, string[]>>({ type: [], input: [], length: [], group: [] });
const AXES = [
  ["change", "Change %"],
  ["time", "Run time"],
] as const satisfies readonly (readonly [ComparisonAxis, string])[];
const HORIZONTAL_AXES = [["commit", "Commit"], ["time", "Time (UTC)"]] as const;
const axis = ref<ComparisonAxis>("change");
const horizontal = ref<HistoryAxis>("commit");
// Brush bounds identify exact reports, not dates: commit order can differ from
// measurement order, so translating a brush into measured dates would select extra runs.
const firstRun = ref("");
const lastRun = ref("");
const STATE_KEYS = ["case", "from", "to", "axis", "x", "first", "last", ...FILTERS.map(({ key }) => key)];
function clearZoom() { firstRun.value = ""; lastRun.value = ""; }

/** Adopt the hash's view; other fragments, such as heading anchors, keep the current one. */
function readHash(initial = false) {
  const params = new URLSearchParams(location.hash.slice(1));
  const owned = STATE_KEYS.some((key) => params.has(key));
  if (!owned && !initial && location.hash) return;
  const ids = (owned ? params.getAll("case") : []).filter(Boolean);
  selection.value = (ids.length || owned ? ids : [props.caseId])
    .slice(0, theme.series.length)
    .map((id, index) => ({ id, color: theme.series[index] }));
  from.value = owned ? (params.get("from") ?? "") : daysAgo(DEFAULT_DAYS);
  to.value = owned ? (params.get("to") ?? "") : "";
  axis.value = params.get("axis") === "time" ? "time" : "change";
  horizontal.value = params.get("x") === "time" ? "time" : "commit";
  firstRun.value = params.get("first") ?? "";
  lastRun.value = params.get("last") ?? "";
  filters.value = Object.fromEntries(
    FILTERS.map(({ facet, key }) => [facet, params.getAll(key)]),
  ) as Record<Facet, string[]>;
}
function writeHash() {
  const params = new URLSearchParams();
  for (const { id } of selection.value) params.append("case", id);
  params.set("from", from.value);
  params.set("to", to.value);
  params.set("axis", axis.value);
  params.set("x", horizontal.value);
  if (firstRun.value) params.set("first", firstRun.value);
  if (lastRun.value) params.set("last", lastRun.value);
  for (const { facet, key } of FILTERS)
    for (const value of filters.value[facet]) params.append(key, value);
  // Replace, not push: exploring should not fill the back button.
  history.replaceState(history.state, "", `#${params}`);
}
const onHashChange = () => readHash();
onMounted(() => {
  readHash(true);
  watch([selection, from, to, filters, axis, horizontal, firstRun, lastRun], writeHash, { deep: true });
  window.addEventListener("hashchange", onHashChange);
});
onUnmounted(() => window.removeEventListener("hashchange", onHashChange));

// The index first, then only the monthly journals the range needs, each once.
const index = shallowRef<ReturnType<typeof indexSchema.parse> | null>(null);
const journals = shallowRef(new Map<string, TrackingRun[]>());
const error = ref("");
const loading = ref(true);

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
    throw new DataError(
      `The performance data could not be downloaded (HTTP ${response.status}). Reload the page to try again.`,
    );
  return response.text();
}
function readable<T>(read: () => T) {
  try {
    return read();
  } catch (cause) {
    throw new DataError("The published performance data has a format this page cannot read.", {
      cause,
    });
  }
}
function fail(cause: unknown) {
  console.error("Performance history:", cause);
  error.value =
    cause instanceof DataError ? cause.message : "The performance data could not be displayed.";
}
const months = computed(() => {
  const start = from.value.slice(0, 7);
  const end = to.value.slice(0, 7);
  return (index.value?.months ?? []).filter(
    (month) => (!start || month >= start) && (!end || month <= end),
  );
});
onMounted(() => {
  watch(
    () => props.source,
    async (source, _, cleanup) => {
      const controller = new AbortController();
      cleanup(() => controller.abort());
      loading.value = true;
      error.value = "";
      index.value = null;
      journals.value = new Map();
      try {
        const text = await download(source, "index.json", controller.signal);
        index.value = readable(() => indexSchema.parse(JSON.parse(text)));
      } catch (cause) {
        if (controller.signal.aborted) return;
        fail(cause);
        loading.value = false;
      }
    },
    { immediate: true },
  );
  watch(months, async (needed, _, cleanup) => {
    const missing = needed.filter((month) => !journals.value.has(month));
    // A new measured range can recover from a failed month, including when all
    // of its journals are already cached. Keep index-fetch failures visible.
    if (index.value) error.value = "";
    if (!index.value || !missing.length) {
      if (index.value) loading.value = false;
      return;
    }
    const controller = new AbortController();
    cleanup(() => controller.abort());
    loading.value = true;
    try {
      const source = props.source;
      const batches = await Promise.all(
        missing.map(async (month) => {
          const journal = await download(source, `${month}.jsonl`, controller.signal);
          return [month, readable(() => parseRuns(journal))] as const;
        }),
      );
      if (controller.signal.aborted) return;
      journals.value = new Map([...journals.value, ...batches]);
    } catch (cause) {
      if (!controller.signal.aborted) fail(cause);
    } finally {
      if (!controller.signal.aborted) loading.value = false;
    }
  });
});

const runs = computed(() => {
  const start = lowerBound(from.value);
  const end = upperBound(to.value);
  const measured = [...journals.value.values()].flat().filter((run) => {
    const time = Date.parse(run.time);
    return time >= start && time <= end;
  });
  return comparableRuns(measured, firstRun.value || lastRun.value
    ? { first: firstRun.value, last: lastRun.value } : undefined);
});
const cases = computed(() => index.value?.cases ?? []);
// An empty filter admits every value; filters intersect.
const shown = computed(() =>
  cases.value.filter((entry) =>
    FILTERS.every(
      ({ facet }) => !filters.value[facet].length || filters.value[facet].includes(entry[facet]),
    ),
  ),
);
function setFilter(facet: Facet, value: string | null) {
  const current = filters.value[facet];
  const next =
    value === null
      ? []
      : current.includes(value)
        ? current.filter((item) => item !== value)
        : [...current, value];
  filters.value = { ...filters.value, [facet]: next };
}
const byId = computed(() => new Map(cases.value.map((entry) => [entry.id, entry])));

// A colour follows its case: a new case takes the first colour not in use.
const limitNotice = ref(false);
const isSelected = (id: string) => selection.value.some((item) => item.id === id);
const full = computed(() => selection.value.length >= theme.series.length);
function toggle(id: string) {
  limitNotice.value = false;
  if (isSelected(id)) {
    selection.value = selection.value.filter((item) => item.id !== id);
    return;
  }
  const color = theme.series.find((name) => !selection.value.some((item) => item.color === name));
  if (!color) {
    limitNotice.value = true;
    return;
  }
  selection.value = [...selection.value, { id, color }];
}
const compared = computed<ComparedCase[]>(() =>
  selection.value.flatMap(({ id, color }) => {
    const entry = byId.value.get(id);
    return entry ? [{ entry, color }] : [];
  }),
);
const swatches = computed(() =>
  Object.fromEntries(
    theme.series.map((name) => [
      `--series-${name}`,
      theme.palette[isDark.value ? "dark" : "light"][name],
    ]),
  ),
);

const preset = computed(() =>
  to.value
    ? null
    : (PRESETS.find(([days]) => (days ? from.value === daysAgo(days) : !from.value))?.[0] ?? null),
);
function choose(days: number) {
  clearZoom();
  from.value = days ? daysAgo(days) : "";
  to.value = "";
}
function changeDate(event: Event, bound: "from" | "to") {
  clearZoom();
  const value = (event.target as HTMLInputElement).value;
  if (bound === "from") from.value = value; else to.value = value;
}

// Re-layout at the host width instead of scaling the whole SVG: wide windows
// gain plot space, not taller charts or larger text. Narrow hosts still shrink
// the renderer's minimum layout through CSS. Observe the host, not the SVG.
const historyElement = useTemplateRef<HTMLElement>("history");
const figureWidth = ref<number>(theme.layout.width);
let resizeObserver: ResizeObserver | undefined;
onMounted(() => {
  resizeObserver = new ResizeObserver(([entry]) => {
    figureWidth.value = Math.floor(entry.contentRect.width);
    hover.value = null;
    drag.value = null;
  });
  resizeObserver.observe(historyElement.value!);
});
onUnmounted(() => resizeObserver?.disconnect());

const trendId = useId();
const compareId = useId();
const link = (id: string) => `#${new URLSearchParams({ case: id })}`;
const caseOf = (anchor: Element) =>
  new URLSearchParams(anchor.getAttribute("href")?.slice(1) ?? "").get("case");
const trend = computed(() =>
  renderMainTrend(runs.value, shown.value, trendId, link, figureWidth.value, horizontal.value),
);
const comparison = computed(() =>
  renderComparison(runs.value, compared.value, axis.value, compareId, figureWidth.value, horizontal.value),
);
const comparedLatest = computed(
  () =>
    new Map(
      latestValues(
        runs.value,
        compared.value.map(({ entry }) => entry),
      ).map((row) => [row.entry.id, row]),
    ),
);
const latest = computed(
  () => new Map(latestValues(runs.value, shown.value).map((row) => [row.entry.id, row])),
);

// Row links are anchors for meaning, but a plain click toggles the case in
// place: the page neither jumps nor records history. `.vp-raw` keeps the
// site router from handling them first.
const overview = useTemplateRef<HTMLElement>("overview");
function onTrendClick(event: MouseEvent) {
  if (event.button || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
  const anchor = (event.target as Element).closest(".trend-row-link a");
  const id = anchor && caseOf(anchor);
  if (!id) return;
  event.preventDefault();
  toggle(id);
}
onMounted(() =>
  watch(
    overview,
    (element, previous) => {
      previous?.removeEventListener("click", onTrendClick);
      element?.addEventListener("click", onTrendClick);
    },
    { immediate: true },
  ),
);
// The figure is an image to assistive technology, so its row links stay out
// of the focus order; the table's checkboxes offer the same selection.
watch(
  [trend, selection],
  async () => {
    await nextTick();
    for (const anchor of overview.value?.querySelectorAll<SVGElement>(".trend-row-link a") ?? []) {
      const color = selection.value.find((item) => item.id === caseOf(anchor))?.color;
      anchor.classList.toggle("selected", Boolean(color));
      anchor.style.setProperty("--series", color ? `var(--series-${color})` : "");
      anchor.setAttribute("tabindex", "-1");
      anchor.setAttribute("aria-hidden", "true");
    }
  },
  { flush: "post", deep: true },
);

// The comparison's readout and range selection map pointer positions through
// the figure's plot frame; the date fields offer the same range by keyboard.
const chart = useTemplateRef<HTMLElement>("chart");
const focus = ref<Color | null>(null);
const hover = ref<number | null>(null);
const drag = ref<{ start: number; end: number } | null>(null);
const scale = () => {
  const svg = chart.value?.querySelector("svg");
  return svg && comparison.value ? svg.clientWidth / comparison.value.frame.width : 0;
};
const positionAt = (pixels: number) => {
  const { left, right, start, end } = comparison.value!.frame;
  const x = Math.min(right, Math.max(left, pixels / scale()));
  return start + ((x - left) / (right - left)) * (end - start);
};
const pixelsAt = (position: number) => {
  const { left, right, start, end } = comparison.value!.frame;
  return (left + ((position - start) / (end - start)) * (right - left)) * scale();
};
const offset = (event: PointerEvent) => event.clientX - chart.value!.getBoundingClientRect().left;
function onPointerMove(event: PointerEvent) {
  if (!comparison.value || !scale()) return;
  if (drag.value) {
    drag.value = { ...drag.value, end: offset(event) };
    return;
  }
  const position = positionAt(offset(event));
  const table = comparison.value.runs;
  let nearest = 0;
  for (let i = 1; i < table.length; i++)
    if (Math.abs(table[i].position - position) <= Math.abs(table[nearest].position - position)) nearest = i;
  hover.value = nearest;
}
function onPointerDown(event: PointerEvent) {
  // Points open their commit; a drag anywhere else selects a time range.
  if (event.button || (event.target as Element).closest("a") || !comparison.value) return;
  drag.value = { start: offset(event), end: offset(event) };
  hover.value = null;
  chart.value?.setPointerCapture(event.pointerId);
}
function onPointerUp() {
  const selected = drag.value;
  drag.value = null;
  if (!selected || Math.abs(selected.end - selected.start) < 8) return;
  const [start, end] = [positionAt(selected.start), positionAt(selected.end)].toSorted((a, b) => a - b);
  const included = comparison.value!.runs.filter((run) => run.position >= start && run.position <= end);
  if (!included.length) return;
  firstRun.value = included[0].id;
  lastRun.value = included.at(-1)!.id;
}
const readout = computed(() => {
  const result = comparison.value;
  const run = hover.value === null ? undefined : result?.runs[hover.value];
  if (!result || !run || drag.value) return null;
  const x = pixelsAt(run.position);
  const s = scale();
  return {
    run,
    line: {
      left: `${x}px`,
      top: `${result.frame.top * s}px`,
      height: `${(result.frame.bottom - result.frame.top) * s}px`,
    },
    // The card sits on the side of the line with more room.
    card: {
      left: `${x}px`,
      transform:
        x > (chart.value?.clientWidth ?? 0) / 2
          ? "translateX(calc(-100% - 12px))"
          : "translateX(12px)",
    },
  };
});
watch(
  [focus, comparison],
  async () => {
    await nextTick();
    for (const group of chart.value?.querySelectorAll<SVGElement>("[class*='series-']") ?? []) {
      const dimmed = focus.value && !group.classList.contains(`series-${focus.value}`);
      group.style.opacity = dimmed ? "0.15" : "";
    }
  },
  { flush: "post" },
);
watch(comparison, () => { hover.value = null; drag.value = null; });
const emptyMessage = computed(() => {
  if (!index.value || loading.value) return "";
  if (!index.value.months.length) return "No main measurements have been published yet.";
  if (!runs.value.length) return firstRun.value || lastRun.value
    ? "The selected reports are unavailable in this measured range. Reset zoom to see the other runs."
    : "No measurements in this range.";
  return "Select rows below to compare their run time.";
});
</script>

<template>
  <section ref="history" class="history" aria-label="Main performance history" :style="swatches">
    <div v-if="index && !error" class="filters">
      <fieldset v-for="{ facet, name } in FILTERS" :key="facet" class="filter" :aria-label="name">
        <span class="filter-name">{{ name }}</span>
        <button
          type="button"
          :aria-pressed="!filters[facet].length"
          @click="setFilter(facet, null)"
        >
          All
        </button>
        <button
          v-for="value in index.facets[facet]"
          :key="value"
          type="button"
          :aria-pressed="filters[facet].includes(value)"
          @click="setFilter(facet, value)"
        >
          {{ value }}
        </button>
      </fieldset>
    </div>
    <div class="panel">
      <div class="toolbar">
        <fieldset class="presets" aria-label="Time range">
          <button
            v-for="[ days, name ] in PRESETS"
            :key="days"
            type="button"
            :aria-pressed="preset === days"
            @click="choose(days)"
          >
            {{ name }}
          </button>
        </fieldset>
        <label class="date">
          <span>Measured from (UTC)</span>
          <input type="date" :value="from.slice(0, 10)" @change="changeDate($event, 'from')">
        </label>
        <label class="date">
          <span>to</span>
          <input type="date" :value="to.slice(0, 10)" @change="changeDate($event, 'to')">
        </label>
        <fieldset class="presets" aria-label="Horizontal axis">
          <legend class="visually-hidden">Horizontal axis</legend>
          <button
            v-for="[value, name] in HORIZONTAL_AXES" :key="value" type="button"
            :aria-pressed="horizontal === value" @click="horizontal = value"
          >{{ name }}</button>
        </fieldset>
        <fieldset class="presets" aria-label="Comparison vertical axis">
          <legend class="visually-hidden">Comparison vertical axis</legend>
          <button
            v-for="[ value, name ] in AXES"
            :key="value"
            type="button"
            :aria-pressed="axis === value"
            @click="axis = value"
          >
            {{ name }}
          </button>
        </fieldset>
        <span v-if="loading" role="status" class="status">Loading…</span>
      </div>
      <p v-if="firstRun || lastRun" class="chart-note" role="status">
        Zoomed to a report range. <button type="button" class="reset-zoom" @click="clearZoom">Reset zoom</button>
      </p>
      <p class="chart-note">
        {{ horizontal === "commit" ? "Commits in timestamp order; reruns keep separate slots." : "Git committer time (UTC); reruns may overlap." }}
        Drag across the chart to zoom.
      </p>
      <ul v-if="compared.length" class="chips" aria-label="Compared cases">
        <li
          v-for="{ entry, color } in compared"
          :key="entry.id"
          @mouseenter="focus = color"
          @mouseleave="focus = null"
        >
          <span class="swatch" :style="{ background: `var(--series-${color})` }" />
          <span>{{ entry.label }}</span>
          <template v-if="comparedLatest.get(entry.id)">
            <span class="value">{{ formatTime(comparedLatest.get(entry.id)!.value) }}</span>
            <span class="value">{{ formatChange(comparedLatest.get(entry.id)!.change) }}</span>
          </template>
          <button type="button" :aria-label="`Remove ${entry.label}`" @click="toggle(entry.id)">
            ×
          </button>
        </li>
        <li v-if="limitNotice" class="limit" role="status">
          Up to {{ theme.series.length }} cases; remove one to add another.
        </li>
      </ul>
      <div v-if="comparison && axis === 'change'" class="chart-note baselines">
        <p>Change = (run time / baseline − 1) × 100%. Each case uses its first comparable report in the selected range, in commit timestamp order.</p>
        <ul aria-label="Percentage baselines">
          <template v-for="({ entry }, i) in compared" :key="entry.id">
            <li v-if="comparison.baselines[i]">
              {{ entry.label }}: 0% = {{ formatTime(comparison.baselines[i]!.value) }} at
              <a :href="`https://github.com/Yikai-Liao/tessera-sort/commit/${comparison.baselines[i]!.hash}`" target="_blank" rel="noreferrer">{{ comparison.baselines[i]!.hash.slice(0, 7) }}</a>
              · measured {{ formatTimestamp(comparison.baselines[i]!.time) }}
            </li>
          </template>
        </ul>
      </div>
      <p v-if="error" class="notice" role="alert">{{ error }}</p>
      <div
        v-else-if="comparison"
        ref="chart"
        class="chart compare"
        :class="{ busy: loading }"
        :aria-busy="loading"
        @pointermove="onPointerMove"
        @pointerleave="hover = null"
        @pointerdown="onPointerDown"
        @pointerup="onPointerUp"
        @pointercancel="drag = null"
      >
        <!-- The shared renderer escapes labels through DOM text nodes. -->
        <div v-html="comparison.svg" />
        <template v-if="readout">
          <div class="crosshair" :style="readout.line" />
          <div class="readout" :style="readout.card">
            <div class="when">
              {{ readout.run.hash.slice(0, 7) }}
              <div>Commit: {{ formatTimestamp(readout.run.commitTime) }}</div>
              <div>Measured: {{ formatTimestamp(readout.run.time) }}</div>
            </div>
            <div v-for="({ entry, color }, i) in compared" :key="entry.id" class="line">
              <span class="swatch" :style="{ background: `var(--series-${color})` }" />
              <span>{{ entry.label }}</span>
              <template v-if="readout.run.values[i]">
                <span class="value">{{ formatTime(readout.run.values[i]!.value) }}</span>
                <span class="value">{{ formatChange(readout.run.values[i]!.change) }}</span>
              </template>
              <span v-else class="value">—</span>
              <span v-if="!readout.run.values[i]" />
              <span v-if="comparison.baselines[i]" class="baseline">
                vs {{ comparison.baselines[i]!.hash.slice(0, 7) }} · measured {{ formatTimestamp(comparison.baselines[i]!.time) }}
              </span>
            </div>
          </div>
        </template>
        <div
          v-if="drag"
          class="brush"
          :style="{
            left: `${Math.min(drag.start, drag.end)}px`,
            width: `${Math.abs(drag.end - drag.start)}px`,
          }"
        />
      </div>
      <p v-else-if="emptyMessage" class="notice">{{ emptyMessage }}</p>
    </div>
    <div
      v-if="trend && !error"
      ref="overview"
      class="chart trend vp-raw"
      :class="{ busy: loading }"
      :aria-busy="loading"
      v-html="trend"
    />
    <p v-if="index && !error && cases.length && !shown.length" class="notice">
      No cases match these filters.
    </p>
    <details v-if="latest.size && !error">
      <summary>Latest values of every case</summary>
      <table>
        <thead>
          <tr>
            <th><span class="visually-hidden">Compare</span></th>
            <th>Case</th>
            <th>Mode</th>
            <th class="number">Latest</th>
            <th class="number">Interval</th>
            <th class="number">Change</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in latest.values()" :key="row.entry.id">
            <td>
              <input
                type="checkbox"
                :checked="isSelected(row.entry.id)"
                :disabled="full && !isSelected(row.entry.id)"
                :aria-label="`Compare ${row.entry.label}`"
                @change="toggle(row.entry.id)"
              >
            </td>
            <td>{{ row.entry.label }}</td>
            <td>{{ row.entry.group }}</td>
            <td class="number">
              {{ formatTime(row.value)
              }}<span v-if="!row.current" class="stale"> ({{ row.time.slice(0, 10) }})</span>
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
.filters {
  display: grid;
  gap: 6px;
  margin-top: 16px;
  font-size: 13px;
}
.filter {
  margin: 0;
  padding: 0;
  min-width: 0;
  border: 0;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px;
}
.filter-name {
  width: 92px;
  color: var(--vp-c-text-2);
}
.filter button {
  padding: 2px 10px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 999px;
  color: var(--vp-c-text-2);
  font-variant-numeric: tabular-nums;
}
.filter button:hover {
  color: var(--vp-c-text-1);
}
.filter button[aria-pressed="true"] {
  border-color: var(--vp-c-text-3);
  background: var(--vp-c-default-soft);
  color: var(--vp-c-text-1);
  font-weight: 600;
}
/* Scroll with the document: a pinned chart can consume most of a zoomed
   desktop viewport and leave too little room to explore the trend rows. */
.panel {
  padding: 16px 0 12px;
  margin-bottom: 16px;
  border-bottom: 1px solid var(--vp-c-divider);
}
.toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 16px;
  margin-bottom: 10px;
  color: var(--vp-c-text-2);
  font-size: 13px;
}
.presets {
  display: inline-flex;
  margin: 0;
  padding: 0;
  min-width: 0;
  border: 1px solid var(--vp-c-divider);
  border-radius: 8px;
  overflow: hidden;
}
.presets button {
  padding: 4px 12px;
  font-size: 13px;
  color: var(--vp-c-text-2);
}
.presets button + button {
  border-left: 1px solid var(--vp-c-divider);
}
.presets button[aria-pressed="true"] {
  background: var(--vp-c-default-soft);
  color: var(--vp-c-text-1);
  font-weight: 600;
}
.date {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.date input {
  padding: 3px 8px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 8px;
  color: var(--vp-c-text-1);
  font-size: 13px;
  font-variant-numeric: tabular-nums;
}
.status {
  color: var(--vp-c-text-3);
}
.chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin: 0 0 10px;
  padding: 0;
  list-style: none;
  font-size: 13px;
}
.chips li {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  margin: 0;
  max-width: 100%;
  flex-wrap: wrap;
  padding: 2px 4px 2px 10px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 999px;
}
.chips li.limit {
  padding-right: 10px;
  border-style: dashed;
  color: var(--vp-c-text-2);
}
.chips li > button {
  width: 22px;
  height: 22px;
  border-radius: 50%;
  color: var(--vp-c-text-3);
  line-height: 1;
}
.chips li > button:hover {
  background: var(--vp-c-default-soft);
  color: var(--vp-c-text-1);
}
.swatch {
  flex: none;
  width: 10px;
  height: 10px;
  border-radius: 50%;
}
.value {
  color: var(--vp-c-text-2);
  font-variant-numeric: tabular-nums;
}
.chart {
  margin: 0 0 20px;
  transition: opacity 0.2s;
}
.panel .chart {
  margin: 0;
}
/* Data from a narrower range may still show while wider months load. */
.chart.busy {
  opacity: 0.5;
}
/* The renderer expands to the host; only narrow columns scale below its minimum. */
.chart :deep(svg) {
  display: block;
  max-width: 100%;
  height: auto;
}
.compare {
  position: relative;
  width: fit-content;
  max-width: 100%;
  cursor: crosshair;
  touch-action: pan-y;
  user-select: none;
}
.crosshair {
  position: absolute;
  width: 1px;
  background: var(--vp-c-text-3);
  pointer-events: none;
}
.brush {
  position: absolute;
  top: 0;
  bottom: 0;
  border-inline: 1px solid var(--vp-c-text-3);
  background: var(--vp-c-default-soft);
  opacity: 0.7;
  pointer-events: none;
}
.readout {
  position: absolute;
  top: 12px;
  z-index: 1;
  padding: 8px 10px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 8px;
  background: var(--vp-c-bg-elv);
  box-shadow: var(--vp-shadow-2);
  font-size: 12px;
  pointer-events: none;
}
.readout .when {
  grid-column: 1 / -1;
  margin-bottom: 2px;
  color: var(--vp-c-text-2);
  font-variant-numeric: tabular-nums;
}
/* One grid for all lines, so values align across cases. */
.readout {
  display: grid;
  grid-template-columns: 10px auto auto auto;
  align-items: center;
  gap: 4px 8px;
  max-width: calc(100vw - 48px);
  white-space: normal;
}
.readout .line {
  display: contents;
}
.readout .value {
  text-align: right;
}
.readout .baseline {
  grid-column: 2 / -1;
  color: var(--vp-c-text-3);
  font-size: 11px;
}
.chart-note {
  margin: 0 0 10px;
  color: var(--vp-c-text-2);
  font-size: 12px;
  line-height: 1.5;
}
.baselines p, .baselines ul {
  margin: 0 0 6px;
}
.baselines ul { padding-left: 16px; }
.baselines li { margin: 0; }
.reset-zoom {
  color: var(--vp-c-brand-1);
  text-decoration: underline;
}
.trend :deep(.trend-row-link a:hover rect) {
  fill: var(--vp-c-default-soft);
}
.trend :deep(.trend-row-link a.selected rect) {
  fill: var(--series);
  fill-opacity: 0.1;
  stroke: var(--series);
  stroke-opacity: 0.6;
}
.notice {
  margin: 0;
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
.visually-hidden {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}
</style>
