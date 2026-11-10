// SPDX-License-Identifier: BSD-2-Clause

import { randomUUID } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { cpus, release } from "node:os";
import { join } from "node:path";
import { CommandCancelledError, command, diagnostic } from "../../tools/process.ts";
import { parseCriterionId } from "../catalog.ts";
import type { Plan } from "../plan.ts";
import { withWorkspace, workingTreeDirty } from "../workspace.ts";
import { assertWorkerEnvironment, buildRevisions, selectedCases } from "./runner.ts";

/** Own a Docker process through failure/cancellation, including daemon-side cleanup. */
export async function runCounterContainer(args: string[]) {
  const name = `tessera-callgrind-${randomUUID()}`;
  try {
    return await command("docker", ["run", "--name", name, "--rm", ...args], { capture: true });
  } catch (error) {
    // Killing the CLI's process group does not kill daemon-owned containers.
    try {
      await command("docker", ["rm", "--force", name], { capture: true });
    } catch (cleanup) {
      diagnostic(`Could not remove counter container ${name}: ${String(cleanup)}`);
    }
    throw error;
  }
}

/** Decode aggregate Callgrind events without depending on function/source annotations. */
export function readCallgrind(text: string): Record<string, number> {
  const events = /^events: (.+)$/m.exec(text)?.[1].trim().split(/\s+/);
  const totals = /^summary: (.+)$/m.exec(text)?.[1].trim().split(/\s+/).map(Number);
  if (
    !events ||
    !totals ||
    totals.length > events.length ||
    new Set(events).size !== events.length ||
    !events.includes("Ir") ||
    totals.some((value) => !Number.isSafeInteger(value) || value < 0) ||
    (totals[events.indexOf("Ir")] ?? 0) <= 0
  )
    throw new Error("Missing or invalid Callgrind event summary");
  return Object.fromEntries(events.map((event, index) => [event, totals[index] ?? 0]));
}

interface CounterCase {
  id: string;
  baseline: Record<string, number>;
  control: Record<string, number>;
  current: Record<string, number>;
}
interface CounterRun {
  schema: 1;
  kind: "callgrind-fixed-sorts";
  baseline: string;
  head: string;
  dirty?: boolean;
  source: Awaited<ReturnType<typeof buildRevisions>>["source"];
  builds: Awaited<ReturnType<typeof buildRevisions>>["builds"];
  binaries: Awaited<ReturnType<typeof buildRevisions>>["hashes"];
  host: { cpu: string; kernel: string };
  valgrind: string;
  image?: string;
  sorts: number;
  selected: string[];
  cases: CounterCase[];
}

/** Regenerate descriptive event ratios; simulated events are not timing verdicts. */
export async function reportCounters(out: string, run?: CounterRun) {
  run ??= JSON.parse(await readFile(join(out, "counters.json"), "utf8"));
  if (
    run?.schema !== 1 ||
    run.kind !== "callgrind-fixed-sorts" ||
    !run.selected.length ||
    new Set(run.selected).size !== run.selected.length ||
    run.cases.map((row) => row.id).join("\n") !== run.selected.join("\n")
  )
    throw new Error("Counter results are incomplete or incompatible");
  const lines = [
    "# Fixed-work instruction and simulated-event comparison",
    "",
    `Baseline: ${run.baseline}. Current head: ${run.head} (${run.dirty === undefined ? "working-tree state unrecorded" : run.dirty ? "modified working tree" : "clean working tree"}). Sorts per process: ${run.sorts}.`,
    "",
    "Collection covers one prepared batch, its loop and dynamic call overhead. Input generation, restoration, warm-up and output verification are excluded. Cache and branch events are simulator results and can depend on addresses. Equal instruction counts do not establish a layout cause or exclude a real latency regression.",
    "",
    "| Case | Event | Baseline | Current | Change | A/A change |",
    "|---|---|---:|---:|---:|---:|",
  ];
  for (const row of run.cases) {
    const events = Object.keys(row.baseline);
    if (
      events.join() !== Object.keys(row.current).join() ||
      events.join() !== Object.keys(row.control).join() ||
      events.some((event) =>
        [row.baseline[event], row.current[event], row.control[event]].some(
          (value) => !Number.isSafeInteger(value) || value < 0,
        ),
      ) ||
      [row.baseline, row.current, row.control].some((counts) => !(counts.Ir > 0))
    )
      throw new Error(`Incompatible counter events for ${row.id}`);
    const percent = (value: number, base: number) =>
      base ? `${((value / base - 1) * 100).toFixed(2)}%` : "n/a";
    for (const event of events)
      lines.push(
        `| ${row.id} | ${event} | ${row.baseline[event]} | ${row.current[event]} | ${percent(row.current[event], row.baseline[event])} | ${percent(row.control[event], row.baseline[event])} |`,
      );
  }
  lines.push(
    "",
    "Raw Callgrind files, compiler/source/executable provenance and the complete event totals are retained alongside this report.",
    "",
  );
  await writeFile(join(out, "index.md"), lines.join("\n"));
}

/** Build shared-harness revisions and collect verified, fixed-size serial sort batches. */
export async function compareCounters(options: {
  root: string;
  out: string;
  baseline: string;
  plan: Plan;
  sorts: number;
  cpu?: string;
  valgrindImage?: string;
  dirty?: boolean;
}) {
  assertWorkerEnvironment();
  if (options.plan.parallel?.length)
    throw new Error("Counter comparisons support serial API plans only");
  if (process.platform !== "linux" || process.arch !== "x64")
    throw new Error("Counter comparisons require Linux x86-64 and Valgrind");
  if (!Number.isSafeInteger(options.sorts) || options.sorts < 1 || options.sorts > 1024)
    throw new Error("Sort count must be 1..1024");
  const { root, out, cpu, sorts } = options;
  const image = options.valgrindImage
    ? await command("docker", ["image", "inspect", "--format", "{{.Id}}", options.valgrindImage], {
        capture: true,
      })
    : undefined;
  const valgrind = image
    ? await runCounterContainer(["--network=none", image, "valgrind", "--version"])
    : await command("valgrind", ["--version"], { capture: true });
  const baseline = await command("git", ["rev-parse", "--verify", `${options.baseline}^{commit}`], {
    cwd: root,
    capture: true,
  });
  const head = await command("git", ["rev-parse", "HEAD"], { cwd: root, capture: true });
  const dirty = options.dirty ?? (await workingTreeDirty(root));
  await mkdir(out, { recursive: false });
  const planPath = join(out, "plan.json");
  await writeFile(planPath, `${JSON.stringify(options.plan, null, 2)}\n`);
  await withWorkspace(root, async (workspace) => {
    const built = await buildRevisions(root, workspace, baseline, ["layout-study"]);
    const selected = await selectedCases(built.binaries, planPath, undefined, workspace.tree);
    if (selected.some((id) => parseCriterionId(id).mode !== "api"))
      throw new Error("Counter comparisons support serial API cases only");
    const run: CounterRun = {
      schema: 1,
      kind: "callgrind-fixed-sorts",
      baseline,
      head,
      dirty,
      valgrind,
      image,
      sorts,
      selected,
      source: built.source,
      builds: built.builds,
      binaries: built.hashes,
      host: { cpu: cpus()[0]?.model ?? "unknown", kernel: release() },
      cases: [],
    };
    const save = () => writeFile(join(out, "counters.json"), `${JSON.stringify(run, null, 2)}\n`);
    await save();
    for (const [index, id] of selected.entries()) {
      const row = { id } as CounterCase;
      for (const role of index % 2
        ? (["current", "control", "baseline"] as const)
        : (["baseline", "control", "current"] as const)) {
        const binary = built.binaries[role === "current" ? "current" : "baseline"];
        const file = join(out, `callgrind-${index}-${role}.out`);
        const args = [
          "--tool=callgrind",
          "--collect-atstart=no",
          "--toggle-collect=tessera_counted_sorts",
          "--cache-sim=yes",
          "--branch-sim=yes",
          `--callgrind-out-file=${file}`,
          binary,
          "--bench",
        ];
        const env = {
          TESSERA_BENCH_PLAN: planPath,
          TESSERA_BENCH_IDS: id,
          TESSERA_BENCH_FIXED_SORTS: String(sorts),
        };
        if (image && [out, workspace.directory].some((path) => path.includes(",")))
          throw new Error("Container mount paths cannot contain commas");
        const dockerArgs = [
          "--network=none",
          "--read-only",
          "--tmpfs",
          "/tmp",
          "--cap-drop=ALL",
          "--security-opt=no-new-privileges",
          "--user",
          `${process.getuid!()}:${process.getgid!()}`,
          "--workdir",
          workspace.tree,
          "--mount",
          `type=bind,src=${workspace.directory},dst=${workspace.directory},readonly`,
          "--mount",
          `type=bind,src=${out},dst=${out}`,
          ...(cpu ? ["--cpuset-cpus", cpu] : []),
          ...Object.entries(env).flatMap(([key, value]) => ["--env", `${key}=${value}`]),
          image ?? "",
          "valgrind",
          ...args,
        ];
        const output = image
          ? await runCounterContainer(dockerArgs)
          : await command(
              cpu ? "taskset" : "valgrind",
              cpu ? ["-c", cpu, "valgrind", ...args] : args,
              {
                cwd: workspace.tree,
                capture: true,
                env,
              },
            );
        const receipt = JSON.parse(output);
        if (receipt.id !== id || receipt.sorts !== sorts)
          throw new Error("Fixed-work receipt differs from request");
        row[role] = readCallgrind(await readFile(file, "utf8"));
      }
      run.cases.push(row);
      await save();
    }
    await reportCounters(out, run);
  });
}

/** Probe each PMU alias independently on this host without changing kernel permissions. */
export async function probeCounters(out: string) {
  if (process.platform !== "linux") throw new Error("PMU probing requires Linux perf");
  await mkdir(out, { recursive: false });
  const version = await command("perf", ["--version"], { capture: true });
  const events = [];
  for (const [index, event] of [
    "instructions:u",
    "branch-misses:u",
    "idq.dsb_uops:u",
    "idq.mite_uops:u",
    "baclears.any:u",
  ].entries()) {
    const file = join(out, `perf-${index}.txt`);
    let error: string | undefined;
    try {
      await command("perf", ["stat", "-x,", "-o", file, "-e", event, "--", "true"]);
    } catch (failure) {
      if (failure instanceof CommandCancelledError) throw failure;
      error = String(failure);
    }
    let output = "";
    try {
      output = await readFile(file, "utf8");
    } catch (failure) {
      if ((failure as NodeJS.ErrnoException).code !== "ENOENT") throw failure;
    }
    const count = output
      .split("\n")
      .find((line) => line.split(",")[2] === event)
      ?.split(",")[0];
    events.push({
      event,
      available: !error && !!count && /^\d+$/.test(count.trim()),
      output,
      ...(error ? { error } : {}),
    });
  }
  const result = {
    scope: "current-host-only",
    perf: version,
    cpu: cpus()[0]?.model,
    kernel: release(),
    events,
  };
  await writeFile(join(out, "probe.json"), `${JSON.stringify(result, null, 2)}\n`);
  return result;
}
