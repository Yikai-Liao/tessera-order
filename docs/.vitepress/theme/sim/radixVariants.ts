// SPDX-License-Identifier: BSD-2-Clause
// Textbook radix sorts on two decimal digits, for the comparison on the
// radix-sort page. Each function returns the array after every pass or level
// and the extra memory that pass needed, counted in records.

export interface Item {
  key: number;
  id: number;
}

export interface Stage {
  label: string;
  items: Item[];
  /** Scratch this stage needs beyond the input. */
  extra: string;
  /** Index ranges sorted independently at this stage (MSD levels). */
  groups?: [number, number][];
}

const tens = (key: number) => Math.floor(key / 10) % 10;
const ones = (key: number) => key % 10;

/** One stable counting pass from `source` into a fresh buffer. */
function countingPass(source: Item[], digit: (key: number) => number): Item[] {
  const counts = new Array(10).fill(0);
  for (const item of source) counts[digit(item.key)]++;
  let sum = 0;
  const starts = counts.map((c) => {
    const start = sum;
    sum += c;
    return start;
  });
  const out = new Array<Item>(source.length);
  for (const item of source) out[starts[digit(item.key)]++] = item;
  return out;
}

/** LSD: least significant digit first, each pass into an n-record buffer. */
export function lsd(input: Item[]): Stage[] {
  const first = countingPass(input, ones);
  const second = countingPass(first, tens);
  return [
    { label: "Input", items: input, extra: "" },
    { label: "Pass 1: ones digit → buffer B", items: first, extra: "a second buffer of n records" },
    {
      label: "Pass 2: tens digit → back to buffer A",
      items: second,
      extra: "the same n-record buffer",
    },
  ];
}

/**
 * Block LSD (Clausecker and Schintke): the same passes as LSD, but each pass
 * fills buckets block by block and recycles consumed input blocks, so the
 * extra memory is O(√n). Only the block table changes between passes; the
 * records are compacted into the input once, at the end.
 */
export function blockLsd(input: Item[]): Stage[] {
  const [, first, second] = lsd(input);
  return [
    { label: "Input", items: input, extra: "" },
    {
      ...first,
      label: "Pass 1: ones digit, block by block",
      extra: "2R blocks + block tables: O(√n)",
    },
    { ...second, label: "Pass 2: tens digit, then compact", extra: "the same blocks and tables" },
  ];
}

/** In-place MSD (American flag sort): swap cycles inside each bucket range. */
function flagPass(a: Item[], start: number, end: number, digit: (key: number) => number) {
  const counts = new Array(10).fill(0);
  for (let i = start; i < end; i++) counts[digit(a[i].key)]++;
  const next: number[] = [];
  const stop: number[] = [];
  let at = start;
  for (const count of counts) {
    next.push(at);
    at += count;
    stop.push(at);
  }
  const groups: [number, number][] = [];
  for (let b = 0; b < 10; b++) {
    if (stop[b] > (b === 0 ? start : stop[b - 1]))
      groups.push([b === 0 ? start : stop[b - 1], stop[b]]);
    while (next[b] < stop[b]) {
      let item = a[next[b]];
      let d = digit(item.key);
      while (d !== b) {
        const target = next[d]++;
        [item, a[target]] = [a[target], item];
        d = digit(item.key);
      }
      a[next[b]++] = item;
    }
  }
  return groups;
}

export function msdInPlace(input: Item[]): Stage[] {
  const a = [...input];
  const groups = flagPass(a, 0, a.length, tens);
  const first = [...a];
  for (const [start, end] of groups) flagPass(a, start, end, ones);
  return [
    { label: "Input", items: input, extra: "" },
    {
      label: "Level 1: tens digit, swapped in place",
      items: first,
      extra: "one counter per bucket",
      groups,
    },
    {
      label: "Level 2: ones digit inside each bucket",
      items: [...a],
      extra: "one counter per bucket, per level",
      groups,
    },
  ];
}

/**
 * Tessera's shape: a stable top-digit split whose scratch is a few blocks,
 * then each bucket is finished on its own in a small tile buffer.
 */
export function tessera(input: Item[]): Stage[] {
  const first = countingPass(input, tens);
  const groups: [number, number][] = [];
  let start = 0;
  for (let d = 0; d < 10; d++) {
    const end = start + first.filter((item) => tens(item.key) === d).length;
    if (end > start) groups.push([start, end]);
    start = end;
  }
  const second: Item[] = [];
  for (const [s, e] of groups) second.push(...countingPass(first.slice(s, e), ones));
  return [
    { label: "Input", items: input, extra: "" },
    {
      label: "Block partition on the tens digit",
      items: first,
      extra: "2R blocks + block tables: O(√n)",
      groups,
    },
    {
      label: "Each bucket finished as a tile",
      items: second,
      extra: "one tile buffer, at most 64 KiB",
      groups,
    },
  ];
}

/** Positions where two equal keys appear out of input order. */
export function instabilities(items: Item[]): Set<number> {
  const bad = new Set<number>();
  items.forEach((item, i) => {
    items.forEach((other, j) => {
      if (j > i && other.key === item.key && other.id < item.id) {
        bad.add(i);
        bad.add(j);
      }
    });
  });
  return bad;
}
