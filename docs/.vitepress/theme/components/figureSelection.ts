// SPDX-License-Identifier: BSD-2-Clause

import { computed, ref, watchPostEffect } from "vue";
import { ink } from "../../../../bench/plot/annotations.ts";
import type { FigureSelection } from "../../../../bench/tracking/result.ts";

const mark = "[data-implementation]";

export function useFigureSelection(
  root: () => HTMLElement | null,
  figures: () => { anchor: string; selection?: FigureSelection }[],
) {
  const pinned = ref<string | null>(null);
  const previewed = ref<string | null>(null);
  const selected = computed(() => previewed.value ?? pinned.value ?? "tessera");

  function preview(target: EventTarget | null) {
    previewed.value =
      (target as Element | null)?.closest(mark)?.getAttribute("data-implementation") ?? null;
  }

  function clear() {
    pinned.value = null;
    previewed.value = null;
  }

  function pin(event: MouseEvent) {
    preview(event.target);
    pinned.value = previewed.value;
  }

  // The pinned implementation's label and colour, from any figure that shows it.
  const pinnedEntry = computed(() => {
    const id = pinned.value;
    if (id === null || id === "tessera") return null;
    const entry = figures()
      .map(({ selection }) => selection?.[id])
      .find((value) => value !== undefined);
    return entry ? { id, label: entry.label, ink: ink[entry.tone] } : null;
  });

  watchPostEffect(() => {
    const id = selected.value;
    const selections = new Map(figures().map(({ anchor, selection }) => [anchor, selection]));
    for (const figure of Array.from(
      root()?.querySelectorAll<HTMLElement>("[data-comparison-figure]") ?? [],
    )) {
      for (const element of Array.from(figure.querySelectorAll(mark))) {
        const matches = element.getAttribute("data-implementation") === id;
        element.classList.toggle("selected", id !== "tessera" && matches);
        element.classList.toggle("dimmed", id !== "tessera" && !matches);
      }

      // Columns that show another implementation take its colour; the headings keep their text.
      // A figure without the selected implementation shows Tessera's values.
      const selection = selections.get(figure.getAttribute("data-comparison-figure")!);
      const values = selection?.[id];
      const showing = id !== "tessera" && values !== undefined;
      figure.toggleAttribute("data-selecting", showing);
      if (showing) figure.style.setProperty("--selection-ink", ink[values.tone]);
      else figure.style.removeProperty("--selection-ink");
      const shown = values ?? selection?.tessera;
      if (!shown) continue;
      for (const cell of Array.from(figure.querySelectorAll("[data-comparison-cell]"))) {
        const row = Number(cell.getAttribute("data-comparison-row"));
        const column = Number(cell.getAttribute("data-comparison-cell"));
        cell.textContent = shown.rows[row][column];
      }
    }
  });

  return { selected, pinnedEntry, preview, pin, clear };
}
