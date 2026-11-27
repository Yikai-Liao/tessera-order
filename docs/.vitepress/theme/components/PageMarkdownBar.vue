<!-- SPDX-License-Identifier: BSD-2-Clause -->
<script setup lang="ts">
import { useData, withBase } from "vitepress";
import { computed, onUnmounted, ref, watch } from "vue";

const LABELS = { idle: "Copy Markdown", copying: "Copying…", copied: "Copied", error: "Copy failed" } as const;
const { page } = useData();
const status = ref<keyof typeof LABELS>("idle");
let timer: ReturnType<typeof setTimeout> | undefined;

const markdownHref = computed(() =>
  page.value.isNotFound || !page.value.relativePath ? "" : withBase(`/${page.value.relativePath}`),
);
watch(markdownHref, () => {
  clearTimeout(timer);
  status.value = "idle";
});
onUnmounted(() => clearTimeout(timer));

async function copyMarkdown() {
  if (!markdownHref.value || status.value === "copying") return;
  status.value = "copying";
  try {
    const res = await fetch(markdownHref.value);
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    await navigator.clipboard.writeText(await res.text());
    status.value = "copied";
  } catch {
    status.value = "error";
  }
  clearTimeout(timer);
  timer = setTimeout(() => { status.value = "idle"; }, 2000);
}
</script>

<template>
  <nav v-if="markdownHref" class="tsr-md-bar" aria-label="Markdown export">
    <button type="button" class="tsr-md-action" :class="{ 'is-copied': status === 'copied', 'is-error': status === 'error' }" :disabled="status === 'copying'" @click="copyMarkdown">
      <span aria-live="polite">{{ LABELS[status] }}</span>
    </button>
    <a class="tsr-md-action" :href="markdownHref" target="_blank" rel="noopener">View Markdown</a>
    <a class="tsr-md-link" :href="withBase('/llms.txt')" target="_blank" rel="noopener">llms.txt</a>
  </nav>
</template>
