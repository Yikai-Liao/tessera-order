// SPDX-License-Identifier: BSD-2-Clause

import { computed, ref, shallowRef, watchPostEffect } from "vue";
import { ink } from "../../../../bench/plot/annotations.ts";
import type { FigureSelection } from "../../../../bench/tracking/result.ts";
import type { ImplementationChoice } from "./implementationChoices.data.ts";

const mark = "[data-implementation]";

function describeSelection(
  figure: HTMLElement,
  label: string,
  rows?: FigureSelection[string]["rows"],
) {
  const description = figure.querySelector<HTMLSpanElement>(".selection-description");
  if (!rows && !description) return;

  const svg = figure.querySelector('svg[role="img"]')!;
  const [originalId] = svg.getAttribute("aria-describedby")!.split(" ");
  if (!rows) {
    svg.setAttribute("aria-describedby", originalId);
    description?.remove();
    return;
  }

  const text = rows.map(([speed, rank], index) => {
    const name = figure.querySelector(`[data-comparison-row-name="${index}"]`)!;
    const group = name.getAttribute("data-comparison-row-group");
    return `${group ? `${group} · ` : ""}${name.textContent}: ${speed}, ${rank}`;
  });
  const element = description ?? figure.ownerDocument.createElement("span");
  element.className = "selection-description";
  element.id = `${originalId}-selection`;
  element.textContent = `Speed and Rank show ${label}'s values. ${text.join("; ")}.`;
  if (!description) figure.append(element);
  svg.setAttribute("aria-describedby", `${originalId} ${element.id}`);
}

export function useFigureSelection(
  root: () => HTMLElement | null,
  figures: () => { anchor: string; selection?: FigureSelection }[],
  implementations: readonly ImplementationChoice[],
) {
  const pinnedId = ref("tessera");
  const previewed = ref<string | null>(null);
  const selected = computed(() => previewed.value ?? pinnedId.value);
  const pinned = computed({
    get: () => pinnedId.value,
    set(id) {
      pinnedId.value = id;
      previewed.value = null;
    },
  });

  function preview(target: EventTarget | null) {
    previewed.value =
      (target as Element | null)?.closest(mark)?.getAttribute("data-implementation") ?? null;
  }

  function clear() {
    pinned.value = "tessera";
  }

  function pin(event: MouseEvent) {
    preview(event.target);
    pinned.value = previewed.value ?? "tessera";
  }

  // The displayed implementation's label and colour, with the pin named during a preview.
  const bar = computed(() => {
    if (pinned.value === "tessera") return null;
    const id = selected.value;
    const entry = implementations.find((entry) => entry.id === id);
    const pin = implementations.find((entry) => entry.id === pinned.value);
    return {
      label: entry?.label ?? id,
      ink: inkOf(id),
      pinnedLabel: id === pinned.value ? null : (pin?.label ?? pinned.value),
    };
  });

  const inkOf = (id: string) =>
    ink[implementations.find((entry) => entry.id === id)?.tone ?? "comparison"];

  const choices = shallowRef<ImplementationChoice[]>([]);
  watchPostEffect(() => {
    // Read tags after Vue renders the figures, including figures without Speed/Rank cells.
    figures();
    const ids = new Set(
      Array.from(root()?.querySelectorAll(mark) ?? [], (element) =>
        element.getAttribute("data-implementation"),
      ),
    );
    choices.value = ids.size
      ? implementations.filter(({ id }) => id === "tessera" || ids.has(id))
      : [];
  });

  watchPostEffect(() => {
    const id = selected.value;
    const label = implementations.find((entry) => entry.id === id)?.label ?? id;
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
      if (showing) figure.style.setProperty("--selection-ink", inkOf(id));
      else figure.style.removeProperty("--selection-ink");
      const shown = values ?? selection?.tessera;
      describeSelection(figure, label, showing ? values.rows : undefined);
      if (!shown) continue;
      for (const cell of Array.from(figure.querySelectorAll("[data-comparison-cell]"))) {
        const row = Number(cell.getAttribute("data-comparison-row"));
        const column = Number(cell.getAttribute("data-comparison-cell"));
        cell.textContent = shown.rows[row][column];
      }
    }
  });

  return { pinned, bar, choices, preview, pin, clear };
}
