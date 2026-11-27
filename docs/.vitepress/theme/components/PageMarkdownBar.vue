<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { useData, withBase } from "vitepress";
import { computed, onUnmounted, ref, watch } from "vue";

const { page } = useData();
const status = ref<"idle" | "copying" | "copied" | "error">("idle");
let resetTimer: ReturnType<typeof setTimeout> | undefined;

const markdownHref = computed(() => {
  if (page.value.isNotFound || !page.value.relativePath) return "";
  return withBase(`/${page.value.relativePath}`);
});

const llmsHref = computed(() => withBase("/llms.txt"));

const copyLabel = computed(() => {
  switch (status.value) {
    case "copying":
      return "Copying…";
    case "copied":
      return "Copied";
    case "error":
      return "Copy failed";
    default:
      return "Copy Markdown";
  }
});

function clearTimer() {
  if (resetTimer !== undefined) {
    clearTimeout(resetTimer);
    resetTimer = undefined;
  }
}

function scheduleReset() {
  clearTimer();
  resetTimer = setTimeout(() => {
    status.value = "idle";
    resetTimer = undefined;
  }, 2000);
}

watch(markdownHref, () => {
  clearTimer();
  status.value = "idle";
});

onUnmounted(clearTimer);

async function copyMarkdown() {
  const url = markdownHref.value;
  if (!url || status.value === "copying") return;
  status.value = "copying";
  try {
    const response = await fetch(url);
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const text = await response.text();
    await navigator.clipboard.writeText(text);
    status.value = "copied";
  } catch {
    status.value = "error";
  }
  scheduleReset();
}
</script>

<template>
  <nav v-if="markdownHref" class="tsr-md-bar" aria-label="Markdown export">
    <button
      type="button"
      class="tsr-md-action"
      :class="{ 'is-copied': status === 'copied', 'is-error': status === 'error' }"
      :disabled="status === 'copying'"
      @click="copyMarkdown"
    >
      <span aria-live="polite">{{ copyLabel }}</span>
    </button>
    <a class="tsr-md-action" :href="markdownHref" target="_blank" rel="noopener">View Markdown</a>
    <a class="tsr-md-link" :href="llmsHref" target="_blank" rel="noopener">llms.txt</a>
  </nav>
</template>
