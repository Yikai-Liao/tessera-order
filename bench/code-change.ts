// SPDX-License-Identifier: BSD-2-Clause

import { spawnSync } from "node:child_process";
import { lstat, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { command } from "../tools/process.ts";
import { copySnapshot, resolveCommit } from "./workspace.ts";

export interface CodeChange {
  baseHash: string;
  hash: string | null;
  baseline: number;
  current: number;
  added: number;
  removed: number;
  files: { path: string; baseline: number; current: number; added: number; removed: number }[];
}

interface Token {
  text: string;
  line: number;
}

// Keep literals opaque so comment markers, braces and attributes inside them
// cannot affect the production-code filter. Rust block comments may nest.
function tokens(source: string) {
  const pattern =
    /\s+|\/\/[^\n]*|\/\*|(?:br|cr|r)#{0,255}"|(?:b|c)?"(?:\\[\s\S]|[^"\\])*"|b?'(?:\\(?:u\{[\da-fA-F_]+\}|x[\da-fA-F]{2}|[^\r\n])|[^'\\\r\n])'|[\p{ID_Start}_][\p{ID_Continue}]*|[\s\S]/guy;
  const result: Token[] = [];
  let line = 0;
  while (pattern.lastIndex < source.length) {
    const match = pattern.exec(source)!;
    let text = match[0];
    const comment = text.startsWith("//") || text === "/*";
    if (text === "/*") {
      let depth = 1;
      while (depth && pattern.lastIndex < source.length) {
        const pair = source.slice(pattern.lastIndex, pattern.lastIndex + 2);
        if (pair === "/*") depth++;
        if (pair === "*/") depth--;
        pattern.lastIndex += pair === "/*" || pair === "*/" ? 2 : 1;
      }
      if (depth) throw new Error("Unterminated Rust comment");
      text = source.slice(match.index, pattern.lastIndex);
    } else if (/^(?:br|cr|r)#*"$/.test(text)) {
      const hashes = text.match(/#*/g)!.find((value) => value.length) ?? "";
      const end = source.indexOf(`"${hashes}`, pattern.lastIndex);
      if (end < 0) throw new Error("Unterminated Rust raw string");
      pattern.lastIndex = end + 1 + hashes.length;
      text = source.slice(match.index, pattern.lastIndex);
    }
    if (!comment && text.trim()) result.push({ text, line });
    line += text.split("\n").length - 1;
  }
  return result;
}

function groupEnd(input: Token[], start: number): number {
  const close = { "(": ")", "[": "]", "{": "}" }[input[start]?.text];
  if (!close) throw new Error("Expected Rust delimiter");
  let i = start + 1;
  while (i < input.length && input[i].text !== close) {
    if (["(", "[", "{"].includes(input[i].text)) i = groupEnd(input, i);
    else i++;
  }
  if (i === input.length) throw new Error("Unbalanced Rust delimiters");
  return i + 1;
}

function rejectTestPaths(input: Token[]) {
  if (
    input.some(
      (token, i) =>
        token.text === "#" && input[i + 1]?.text === "[" && input[i + 2]?.text === "path",
    )
  )
    throw new Error("Path-attributed test modules are unsupported in engine line analysis");
}

// Evaluate test=false, leaving features and target predicates unknown. A cfg
// is test-only only when it cannot be enabled in any production configuration.
function productionCfg(input: Token[]): boolean | undefined {
  if (input.at(-1)?.text === ",") input = input.slice(0, -1);
  const text = input.map((token) => token.text);
  if (text.length === 1 && text[0] === "test") return false;
  if (!["all", "any", "not"].includes(text[0]) || text[1] !== "(") return undefined;
  const values: (boolean | undefined)[] = [];
  let start = 2;
  for (let i = start; i < input.length - 1; i++) {
    if (["(", "[", "{"].includes(text[i])) i = groupEnd(input, i) - 1;
    else if (text[i] === ",") {
      values.push(productionCfg(input.slice(start, i)));
      start = i + 1;
    }
  }
  if (start < input.length - 1) values.push(productionCfg(input.slice(start, -1)));
  if (text[0] === "not") return values[0] === undefined ? undefined : !values[0];
  if (text[0] === "all")
    return values.includes(false)
      ? false
      : values.every((value) => value === true)
        ? true
        : undefined;
  return values.includes(true)
    ? true
    : values.every((value) => value === false)
      ? false
      : undefined;
}

/** Normalize physical code lines, excluding comments and explicitly test-only items.
 * External test modules are returned to the snapshot owner, which omits their files.
 * This is a source metric across features, not a count of one compiled configuration.
 */
export function productionLines(source: string) {
  const input = tokens(source);
  const kept: Token[] = [];
  const testModules: string[] = [];
  let braceDepth = 0;
  for (let i = 0; i < input.length; ) {
    const start = i;
    let testOnly = false;
    let inner = false;
    let customPath = false;
    while (input[i]?.text === "#") {
      const bracket = input[i + 1]?.text === "!" ? i + 2 : i + 1;
      if (input[bracket]?.text !== "[") break;
      const end = groupEnd(input, bracket);
      const attr = input.slice(bracket + 1, end - 1);
      if (attr[0]?.text === "cfg_attr" && attr.some((token) => token.text === "test"))
        throw new Error("Test-dependent cfg_attr is unsupported in engine line analysis");
      customPath ||= attr[0]?.text === "path";
      if (
        (attr[0]?.text === "cfg" && productionCfg(attr.slice(2, -1)) === false) ||
        attr[0]?.text === "test"
      ) {
        testOnly = true;
        inner ||= bracket === i + 2;
      }
      i = end;
    }
    if (!testOnly) {
      kept.push(...input.slice(start, i + 1));
      if (input[i]?.text === "{") braceDepth++;
      if (input[i]?.text === "}") braceDepth--;
      i++;
      continue;
    }
    // These forms need Rust's module-path rules or enclosing-item ownership.
    // Decline the metric rather than silently count tests as production code.
    if (inner) {
      if (braceDepth)
        throw new Error("Nested inner test cfg is unsupported in engine line analysis");
      rejectTestPaths(input);
      return { lines: [], testModules, testOnly: true };
    }
    while (i < input.length) {
      const text = input[i].text;
      if (text === "mod") {
        if (braceDepth || customPath)
          throw new Error(
            "Nested or path-attributed test modules are unsupported in engine line analysis",
          );
        testModules.push(input[i + 1].text);
      }
      if (text === ";") {
        i++;
        break;
      }
      if (text === "{") {
        const end = groupEnd(input, i);
        rejectTestPaths(input.slice(i, end));
        i = end;
        if (input[i]?.text === ";") i++;
        break;
      }
      i = text === "(" || text === "[" ? groupEnd(input, i) : i + 1;
    }
  }
  const lines: string[] = [];
  for (const token of kept)
    for (const [offset, part] of token.text.split("\n").entries()) {
      if (part.trim()) lines[token.line + offset] = `${lines[token.line + offset] ?? ""} ${part}`;
    }
  return { lines: lines.filter(Boolean).map((line) => line.trim()), testModules };
}

const SOURCE = "rust/tessera-sort/src/";

// Runtime engine mechanisms, including key classification, SIMD/scalar scans
// and panic recovery. API entry points and figure instrumentation stay outside.
const ENGINE_SOURCE =
  /^rust\/tessera-sort\/src\/(engine|block|key|portable_simd|scan|unwind)(\.rs$|\/.*\.rs$)/;

async function snapshot(root: string, hash: string | null, out: string) {
  const git = (args: string[]) => command("git", args, { cwd: root, capture: true });
  const entries = hash
    ? (await git(["ls-tree", "-r", "-z", hash, "--", SOURCE])).split("\0").filter(Boolean)
    : (await git(["ls-files", "--cached", "--others", "--exclude-standard", "-z", "--", SOURCE]))
        .split("\0")
        .filter(Boolean);
  const files = new Map<string, string[]>();
  const excluded: string[] = [];
  for (const entry of new Set(entries)) {
    const path = hash ? entry.slice(entry.indexOf("\t") + 1) : entry;
    if (!ENGINE_SOURCE.test(path)) continue;
    let source: string;
    if (hash) {
      if (!/^100(644|755) blob /.test(entry))
        throw new Error(`Expected regular engine source: ${path}`);
      source = await git(["show", `${hash}:${path}`]);
    } else {
      const target = join(root, path);
      const stat = await lstat(target).catch((error: NodeJS.ErrnoException) => {
        if (error.code === "ENOENT") return null;
        throw error;
      });
      if (!stat) continue;
      if (!stat.isFile()) throw new Error(`Expected regular engine source: ${path}`);
      const frozen = join(out, "input", path.slice(SOURCE.length));
      await mkdir(dirname(frozen), { recursive: true });
      await copySnapshot(target, frozen, root);
      source = await readFile(frozen, "utf8");
    }
    const { lines, testModules, testOnly } = productionLines(source);
    files.set(path.slice(SOURCE.length), lines);
    const moduleDir = path.endsWith("/mod.rs") ? dirname(path) : path.slice(0, -3);
    if (testOnly) excluded.push(moduleDir.slice(SOURCE.length));
    for (const name of testModules) excluded.push(`${moduleDir}/${name}`.slice(SOURCE.length));
  }
  for (const [path, lines] of files) {
    if (excluded.some((prefix) => path === `${prefix}.rs` || path.startsWith(`${prefix}/`))) {
      files.delete(path);
      continue;
    }
    const target = join(out, path);
    await mkdir(dirname(target), { recursive: true });
    await writeFile(target, lines.length ? `${lines.join("\n")}\n` : "");
  }
  return files;
}

/** Compare core engine production lines at Git revisions or against the working tree
 * (when currentRef is omitted), using Git's line diff after filtering both snapshots.
 * Working-tree input includes tracked and unignored untracked files. No checkout,
 * build or network is needed; results identify resolved commits, or null for the tree.
 * Missing source or unsupported test syntax rejects; callers retain the benchmark verdict.
 */
export async function codeChange(
  baseRef: string,
  currentRef?: string,
  root = process.cwd(),
): Promise<CodeChange> {
  const baseHash = await resolveCommit(root, baseRef);
  const hash = currentRef === undefined ? null : await resolveCommit(root, currentRef);
  const temporary = await mkdtemp(join(tmpdir(), "tessera-code-change-"));
  try {
    const baseline = await snapshot(root, baseHash, join(temporary, "baseline"));
    const current = await snapshot(root, hash, join(temporary, "current"));
    const files: CodeChange["files"] = [];
    for (const path of new Set([...baseline.keys(), ...current.keys()])) {
      const before = baseline.get(path) ?? [];
      const after = current.get(path) ?? [];
      const file = { path, baseline: before.length, current: after.length, added: 0, removed: 0 };
      files.push(file);
      if (JSON.stringify(before) === JSON.stringify(after)) continue;
      const diff = spawnSync(
        "git",
        [
          "diff",
          "--no-index",
          "--no-ext-diff",
          "--no-textconv",
          "--numstat",
          "--",
          baseline.has(path) ? join(temporary, "baseline", path) : "/dev/null",
          current.has(path) ? join(temporary, "current", path) : "/dev/null",
        ],
        { encoding: "utf8", cwd: root },
      );
      if (diff.error || diff.status !== 1)
        throw diff.error ?? new Error(`Code diff failed: ${diff.stderr}`);
      const [added, removed] = diff.stdout.split("\t").map(Number);
      if (!Number.isInteger(added) || !Number.isInteger(removed))
        throw new Error("Invalid code diff");
      file.added = added;
      file.removed = removed;
    }
    return {
      baseHash,
      hash,
      baseline: [...baseline.values()].reduce((sum, lines) => sum + lines.length, 0),
      current: [...current.values()].reduce((sum, lines) => sum + lines.length, 0),
      added: files.reduce((sum, file) => sum + file.added, 0),
      removed: files.reduce((sum, file) => sum + file.removed, 0),
      files: files.sort(
        (a, b) => b.added + b.removed - a.added - a.removed || a.path.localeCompare(b.path),
      ),
    };
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

/** Show totals and changed files; detailed local output also includes unchanged files. */
export function codeTable(
  change: CodeChange,
  options: { baseline: string; current: string; details?: boolean },
) {
  const signed = (value: number) => `${value > 0 ? "+" : value < 0 ? "−" : ""}${Math.abs(value)}`;
  const cell = (text: string) => text.replaceAll("|", "\\|").replace(/[\r\n]/g, " ");
  const row = (
    label: string,
    value: Pick<CodeChange, "baseline" | "current" | "added" | "removed">,
  ) =>
    `| ${label} | ${value.baseline} | ${value.current} | ${value.added} | ${value.removed} | ${signed(value.current - value.baseline)} |`;
  return [
    `| Production LOC | ${cell(options.baseline)} | ${cell(options.current)} | Added | Removed | Net |`,
    "|---|---:|---:|---:|---:|---:|",
    row("Core engine", change),
    ...change.files
      .filter((file) => options.details || file.added || file.removed)
      .map((file) => row(`\`${cell(file.path)}\``, file)),
  ].join("\n");
}
