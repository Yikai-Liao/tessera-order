// SPDX-License-Identifier: BSD-2-Clause

import { chartStyles, escapeXml } from "../../.github/scripts/bencher/svg.ts";
import { type Dataset, geomean, key, lengths, median, type Summary, shapes } from "./model.ts";

export interface Figure {
  name: string;
  title: string;
  svg: string;
  table: string;
}
const fmt = (n: number) => (n >= 100 ? n.toFixed(0) : n.toFixed(2));
const bytes = (n: number) =>
  n >= 1e6 ? `${fmt(n / 1e6)} MB` : n >= 1e3 ? `${fmt(n / 1e3)} kB` : `${fmt(n)} B`;
const text = (x: number, y: number, value: string, cls = "muted", anchor = "start") =>
  `<text x="${x}" y="${y}" class="${cls}" text-anchor="${anchor}">${escapeXml(value)}</text>`;
function svg(title: string, height: number, body: string) {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="720" height="${height}" viewBox="0 0 720 ${height}" role="img" aria-label="${escapeXml(title)}"><title>${escapeXml(title)}</title>${chartStyles}<rect width="720" height="${height}" rx="8" class="panel"/>${body}</svg>\n`;
}
function table(headers: string[], rows: string[][]) {
  return `| ${headers.join(" | ")} |\n| ${headers.map(() => "---").join(" | ")} |\n${rows.map((r) => `| ${r.join(" | ")} |`).join("\n")}\n`;
}
const id = (ty: string, shape: string, n: number) => `${ty}/${shape}/${n}/s1`;
function speed(summaries: Map<string, Summary>, variant: string, n: number) {
  const values = shapes.map((shape) => {
    const c = id("u64", shape, n);
    const build = variant === "tessera + simd" ? "portable-simd" : "default";
    const std = summaries.get(key(build, c, "std-stable")),
      s = summaries.get(key(build, c, variant === "tessera + simd" ? "tessera" : variant));
    return std && s
      ? variant.startsWith("tessera")
        ? std.ratio
        : median(std.ratios.map((r, i) => r / s.ratios[i]))
      : null;
  });
  return values.every((v): v is number => v !== null) ? geomean(values) : null;
}
function ranking(data: Dataset, summaries: Map<string, Summary>): Figure | null {
  const ns = lengths.slice(1);
  const variants = data.manifests
    .get("default")!
    .variants.map((v) => ({ ...v, speeds: ns.map((n) => speed(summaries, v.name, n)) }));
  if (data.manifests.has("portable-simd"))
    variants.push({
      name: "tessera + simd",
      stable: true,
      speeds: ns.map((n) => speed(summaries, "tessera + simd", n)),
    });
  const rows = variants
    .filter((v) => v.speeds.every((n): n is number => n !== null))
    .sort((a, b) => b.speeds[2]! - a.speeds[2]!);
  if (!rows.length) return null;
  const all = rows.flatMap((r) => r.speeds as number[]),
    low = Math.min(0.5, ...all),
    high = Math.max(2, ...all);
  const x = (v: number, column: number) =>
    184 + column * 147 + (Math.log(v / low) / Math.log(high / low)) * 118;
  const height = 172 + rows.length * 35;
  let body = text(24, 30, "Speed relative to slice::sort", "ink strong");
  const tessera = rows.find((r) => r.name === "tessera");
  if (tessera) {
    const competitors = [
      rows.find((r) => r.name === "std-stable"),
      rows.find((r) => !r.stable),
      rows.find((r) => r.stable && !r.name.startsWith("tessera")),
    ];
    body += text(
      24,
      56,
      competitors
        .filter((r) => r)
        .map((r) => `${fmt(tessera.speeds[2]! / r!.speeds[2]!)}× vs ${r!.name}`)
        .join("  ·  "),
      "ink val",
    );
  }
  ns.forEach((n, j) => {
    body += text(242 + 147 * j, 88, n.toLocaleString("en-US"), "muted sub", "middle");
    for (const tick of [0.5, 1, 2, 4, 8, 16])
      if (tick >= low && tick <= high) {
        body +=
          `<path d="M${x(tick, j)} 103V${height - 51}" class="rule"/>` +
          text(x(tick, j), height - 34, `${tick}×`, "muted tiny", "middle");
      }
    if (tessera)
      body += `<path d="M${x(tessera.speeds[j]!, j)} 103V${height - 51}" stroke="#4a80e6" stroke-opacity=".4"/>`;
  });
  body += text(697, 88, "extra memory", "muted sub", "end");
  const plotted: string[][] = [];
  rows.forEach((r, i) => {
    const y = 123 + i * 35,
      highlight = r.name.startsWith("tessera");
    body += text(24, y + 4, r.name, highlight ? "faster strong val" : "muted val");
    (r.speeds as number[]).forEach((v, j) => {
      body += `<path d="M${x(1, j)} ${y}H${x(v, j)}" class="${highlight ? "stem-faster" : "axis"}"/><circle cx="${x(v, j)}" cy="${y}" r="4" class="${highlight ? "faster" : r.stable ? "muted" : "within"}"/>`;
    });
    const build = r.name === "tessera + simd" ? "portable-simd" : "default";
    const memory = data.memory.filter(
      (m) =>
        m.build === build &&
        m.case_id.startsWith("u64/") &&
        m.case_id.endsWith("/10000000/s1") &&
        m.variant === (build === "portable-simd" ? "tessera" : r.name),
    );
    const peak =
      memory.length === shapes.length
        ? bytes(Math.max(...memory.map((m) => m.peak_extra_bytes)))
        : "unmeasured";
    body += text(697, y + 4, peak, "muted sub", "end");
    plotted.push([
      r.name,
      r.stable ? "stable" : "unstable",
      ...(r.speeds as number[]).map((v) => `${fmt(v)}×`),
      peak,
    ]);
  });
  body += text(
    24,
    height - 13,
    "● stable   ○ unstable · u64 keys · geometric mean over 10 shapes · higher is faster",
    "muted sub",
  );
  return {
    name: "ranking",
    title: "Speed ranking",
    svg: svg("Speed ranking", height, body),
    table: table(["Variant", "Stability", ...ns.map(String), "Peak heap at 10M"], plotted),
  };
}
function grid(data: Dataset, summaries: Map<string, Summary>, stable: boolean): Figure | null {
  const panels = stable ? ["u64", "row_u64", "record128", "owned"] : ["u32", "u64", "f64"];
  const title = `Against the fastest ${stable ? "other stable" : "unstable"} sort`;
  const height = 74 + Math.ceil(panels.length / 2) * 390;
  let body = text(24, 30, title, "ink strong");
  const plotted: string[][] = [];
  for (const [p, ty] of panels.entries()) {
    const ox = 18 + (p % 2) * 354,
      oy = 65 + Math.floor(p / 2) * 390;
    body += text(ox, oy, ty, "ink strong");
    lengths.forEach((n, j) => {
      body += text(
        ox + 116 + j * 61,
        oy + 23,
        n === 1000 ? "1K" : n === 65536 ? "65K" : n === 1000000 ? "1M" : "10M",
        "muted sub",
        "middle",
      );
    });
    shapes.forEach((shape, i) => {
      const y = oy + 47 + i * 31;
      body += text(ox, y + 4, shape, "muted sub");
      lengths.forEach((n, j) => {
        const c = id(ty, shape, n),
          x = ox + 88 + j * 61;
        const opponents = [...summaries.values()]
          .filter(
            (s) =>
              s.build === "default" &&
              s.caseId === c &&
              s.stable === stable &&
              !s.variant.startsWith("tessera"),
          )
          .sort((a, b) => a.ratio - b.ratio);
        const winner = opponents[0];
        if (!winner) {
          body += text(x + 28, y + 4, "—", "muted sub", "middle");
          return;
        }
        const cls =
          winner.verdict === "within noise"
            ? "#8d8c86"
            : winner.verdict === "slower"
              ? "#4a80e6"
              : "#e8622f";
        const intensity =
          winner.verdict === "within noise"
            ? 0.1
            : 0.12 + 0.35 * Math.min(1, Math.abs(Math.log(winner.ratio)) / Math.log(4));
        body += `<rect x="${x}" y="${y - 15}" width="56" height="29" rx="3" fill="${cls}" fill-opacity="${intensity}"/>`;
        body += text(x + 28, y - 2, `${fmt(winner.ratio)}×`, "ink sub", "middle");
        // Full names are retained in the value table below the figure.
        body += text(
          x + 28,
          y + 9,
          winner.variant.length > 10 ? winner.variant.slice(0, 9) + "…" : winner.variant,
          "muted tiny",
          "middle",
        );
        plotted.push([
          ty,
          shape,
          String(n),
          winner.variant,
          `${fmt(winner.ratio)}×`,
          winner.verdict === "within noise"
            ? "within noise"
            : winner.verdict === "slower"
              ? "Tessera faster"
              : "Tessera slower",
        ]);
      });
    });
  }
  if (!plotted.length) return null;
  body += text(
    24,
    height - 18,
    "Blue: Tessera faster · orange: Tessera slower · grey: within A/A noise · ratio = opponent / Tessera",
    "muted sub",
  );
  if (!stable && data.manifests.has("portable-simd"))
    body += text(
      24,
      49,
      "Portable SIMD values and the slice::sort anchor are in the full tables.",
      "muted sub",
    );
  return {
    name: stable ? "stable-grid" : "unstable-grid",
    title,
    svg: svg(title, height, body),
    table: table(["Type", "Shape", "Length", "Opponent", "Time / Tessera", "Verdict"], plotted),
  };
}
function lines(data: Dataset, summaries: Map<string, Summary>, memory: boolean): Figure | null {
  const panels = memory ? ["u64", "record128"] : ["u64", "row_u64", "record128", "owned"];
  const title = memory ? "Peak extra heap memory" : "Scaling on random input";
  let body = text(24, 30, title, "ink strong");
  const height = 65 + panels.length * 300,
    plotted: string[][] = [];
  panels.forEach((ty, panel) => {
    const top = 65 + panel * 300;
    const variants = data.manifests.get("default")!.variants.filter((v) => memory || v.stable);
    const series = variants
      .map((v) => ({
        ...v,
        points: lengths.flatMap((n) => {
          const c = id(ty, "random", n),
            s = summaries.get(key("default", c, v.name));
          const m = data.memory.find(
            (m) => m.build === "default" && m.case_id === c && m.variant === v.name,
          );
          const value = memory ? m?.peak_extra_bytes : s?.ns;
          return value === undefined ? [] : [{ n, value }];
        }),
      }))
      .filter((s) => s.points.length);
    if (!series.length) return;
    const size = ty === "record128" ? 128 : 8;
    if (memory)
      series.push({
        name: "input size",
        stable: true,
        points: lengths
          .filter((n) => ty !== "record128" || n <= 1000000)
          .map((n) => ({ n, value: n * size })),
      });
    const values = series.flatMap((s) => s.points.map((p) => Math.max(1, p.value)));
    const low = memory ? 1 : Math.min(...values) * 0.8,
      high = Math.max(...values) * 1.25;
    const x = (n: number) => 68 + (Math.log10(n / 1000) / 4) * 432;
    const y = (v: number) =>
      top +
      205 -
      (memory ? Math.log(Math.max(1, v) / low) / Math.log(high / low) : (v - low) / (high - low)) *
        170;
    body += text(
      24,
      top,
      `${ty} · ${memory ? "bytes (log; zero shown at 1 B)" : "nanoseconds / element"}`,
      "ink strong",
    );
    const ticks = memory
      ? [1, 100, 10000, 1000000, 100000000, 10000000000].filter((v) => v <= high)
      : Array.from({ length: 4 }, (_, j) => low + ((high - low) * j) / 3);
    for (const value of ticks) {
      body +=
        `<path d="M68 ${y(value)}H500" class="rule"/>` +
        text(58, y(value) + 4, memory ? bytes(value) : fmt(value), "muted tiny", "end");
    }
    for (const n of lengths)
      body += text(
        x(n),
        top + 225,
        n === 1000 ? "1K" : n === 65536 ? "65K" : n === 1000000 ? "1M" : "10M",
        "muted sub",
        "middle",
      );
    const labelPositions = series
      .map((s) => ({ s, y: y(s.points.at(-1)!.value) }))
      .sort((a, b) => a.y - b.y);
    labelPositions.forEach((p, j) => {
      p.y = Math.max(p.y, top + 24 + j * 15, j ? labelPositions[j - 1].y + 15 : 0);
    });
    series.forEach((s) => {
      const blue = s.name === "tessera",
        input = s.name === "input size";
      const color = blue ? "#4a80e6" : s.name === "std-stable" ? "#6b6a66" : "#8d8c86";
      const points = s.points.map((p) => `${x(p.n)},${y(p.value)}`).join(" ");
      body += `<polyline points="${points}" fill="none" stroke="${color}" stroke-width="${blue ? 2.4 : 1.1}" ${input ? 'stroke-dasharray="4 3"' : ""}/>`;
      s.points.forEach((p) => {
        body += `<circle cx="${x(p.n)}" cy="${y(p.value)}" r="${blue ? 3 : 2}" fill="${color}"/>`;
        plotted.push([ty, s.name, String(p.n), memory ? String(p.value) : fmt(p.value)]);
      });
      const p = s.points.at(-1)!,
        labelY = labelPositions.find((l) => l.s === s)!.y;
      body +=
        `<path d="M${x(p.n) + 4} ${y(p.value)}L515 ${labelY - 3}" fill="none" stroke="${color}" stroke-opacity=".5"/>` +
        text(522, labelY, s.name, blue ? "faster strong sub" : "muted sub");
    });
  });
  if (!plotted.length) return null;
  return {
    name: memory ? "memory" : "scaling",
    title,
    svg: svg(title, height, body),
    table: table(["Type", "Variant", "Length", memory ? "Peak bytes" : "ns / element"], plotted),
  };
}
export function figures(data: Dataset, summaries: Map<string, Summary>): Figure[] {
  return [
    ranking(data, summaries),
    grid(data, summaries, true),
    grid(data, summaries, false),
    lines(data, summaries, false),
    lines(data, summaries, true),
  ].filter((f): f is Figure => f !== null);
}
