// SPDX-License-Identifier: BSD-2-Clause

export const MARKER = "tessera-bench-report";

/** Identify a benchmark report for one language and exact source revision. */
export function marker(language: string, hash: string) {
  return `<!-- ${MARKER} language=${language} sha=${hash} -->`;
}

/** Retain the active check set and workflow attempt on every published check. */
export function publicationMarker(configurations: string[], attempt: number) {
  return `<!-- tessera-bench-publication ${JSON.stringify({ configurations, attempt })} -->`;
}

/** Read publication metadata; malformed evidence retains the legacy fallback. */
export function readPublication(summary: string) {
  const match = summary.match(/<!-- tessera-bench-publication (\{[^\n]*\}) -->/);
  if (!match) return undefined;
  try {
    const value = JSON.parse(match[1]);
    const ids: unknown = value.configurations;
    return Array.isArray(ids) &&
      ids.length &&
      ids.every((id) => typeof id === "string") &&
      new Set(ids).size === ids.length &&
      typeof value.attempt === "number" &&
      Number.isSafeInteger(value.attempt) &&
      value.attempt > 0
      ? { configurations: ids as string[], attempt: value.attempt as number }
      : undefined;
  } catch {
    return undefined;
  }
}
