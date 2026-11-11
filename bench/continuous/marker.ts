// SPDX-License-Identifier: BSD-2-Clause

export const MARKER = "tessera-bench-report";

/** Identify a benchmark report for one language and exact source revision. */
export function marker(language: string, hash: string) {
  return `<!-- ${MARKER} language=${language} sha=${hash} -->`;
}

/** Retain the active check set on every check in one publication. */
export function configurationMarker(ids: string[]) {
  return `<!-- tessera-bench-configurations ${JSON.stringify(ids)} -->`;
}

/** Read publication metadata; unknown or malformed evidence remains missing. */
export function readConfigurations(summary: string) {
  const match = summary.match(/<!-- tessera-bench-configurations (\[[^\n]*\]) -->/);
  if (!match) return undefined;
  try {
    const ids: unknown = JSON.parse(match[1]);
    return Array.isArray(ids) &&
      ids.length &&
      ids.every((id) => typeof id === "string") &&
      new Set(ids).size === ids.length
      ? (ids as string[])
      : undefined;
  } catch {
    return undefined;
  }
}
