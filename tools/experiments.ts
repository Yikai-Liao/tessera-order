// SPDX-License-Identifier: BSD-2-Clause

import { access } from "node:fs/promises";
import { join, resolve } from "node:path";
import { type Command, InvalidArgumentError, Option } from "commander";
import { stringify as stringifyToml } from "smol-toml";
import { cpuListSchema } from "../bench/affinity.ts";
import { compareCounters, reportCounters } from "../bench/local/counters.ts";
import { LAYOUT_CONTROLS, type LayoutControl, parseLayoutSeeds } from "../bench/local/layout.ts";
import { renderLocalReport } from "../bench/local/report.ts";
import { compareLocal, comparePrepared } from "../bench/local/runner.ts";
import { loadPlan, planSchema, type SelectionOptions } from "../bench/plan.ts";
import { checkAlgorithms, compareAlgorithms, resumeComparison } from "../bench/release.ts";
import { writeReport } from "../bench/report/index.ts";
import { workingTreeDirty } from "../bench/workspace.ts";
import { repositoryRoot, resultDirectory, runTask } from "./tasks.ts";

interface MeasurementOptions extends SelectionOptions {
  plan?: string;
  out?: string;
  cpu?: string;
}

function parseLogicalCpu(value: string) {
  if (!cpuListSchema.safeParse(value).success || process.platform !== "linux")
    throw new InvalidArgumentError("expected a Linux CPU list, such as 2 or 0,2,4,6");
  return value;
}

function parsePositiveInteger(value: string) {
  const number = Number(value);
  if (!Number.isSafeInteger(number) || number < 1)
    throw new InvalidArgumentError("expected a positive integer");
  return number;
}

function parseShard(value: string) {
  const match = /^([1-9]\d*)\/([1-9]\d*)$/.exec(value);
  if (!match || Number(match[1]) > Number(match[2]) || Number(match[2]) > 64)
    throw new InvalidArgumentError("expected index/count with 1 <= index <= count <= 64");
  return { index: Number(match[1]), count: Number(match[2]) };
}

function parseCasePattern(value: string) {
  try {
    new RegExp(value);
  } catch {
    throw new InvalidArgumentError("expected a valid case ID regular expression");
  }
  return value;
}

function parseList(value: string) {
  const values = value.split(",").map((item) => item.trim());
  if (values.some((item) => !item))
    throw new InvalidArgumentError("expected comma-separated values");
  return values;
}

function parseLengths(value: string) {
  const values = parseList(value).map(Number);
  if (values.some((item) => !Number.isSafeInteger(item) || item < 1))
    throw new InvalidArgumentError("expected positive integer lengths");
  return values;
}

function addSelectionOptions(command: Command) {
  return command
    .option(
      "--plan <name-or-file>",
      "JSON/TOML plan or local/continuous/smoke/release/parallel preset",
    )
    .option(
      "--algorithms <ids>",
      "comma-separated algorithms (required controls are retained)",
      parseList,
    )
    .option("--types <ids>", "comma-separated record types", parseList)
    .option("--distributions <ids>", "comma-separated input distributions", parseList)
    .option("--lengths <numbers>", "comma-separated positive record counts", parseLengths);
}

function selectAxes(options: MeasurementOptions): SelectionOptions {
  return Object.fromEntries(
    ["algorithms", "types", "distributions", "lengths"]
      .filter((key) => options[key as keyof MeasurementOptions] !== undefined)
      .map((key) => [key, options[key as keyof MeasurementOptions]]),
  );
}

const exists = (path: string) =>
  access(path).then(
    () => true,
    (error: NodeJS.ErrnoException) => (error.code === "ENOENT" ? false : Promise.reject(error)),
  );

/** Regenerate a report from its persisted result format without rerunning measurements. */
async function report(directories: string[], partial = false) {
  const paths = directories.map((path) => resolve(path));
  const single = paths.length === 1 && !partial;
  if (await exists(join(paths[0], "counters.json"))) {
    if (!single) throw new Error("Counter reports require one complete result directory");
    return reportCounters(paths[0]);
  }
  if (await exists(join(paths[0], "local.json"))) {
    if (!single)
      throw new Error(
        "Revision reports require one directory and complete recorded samples per layout",
      );
    return renderLocalReport(paths[0]);
  }
  return writeReport(paths, partial);
}

/** Register experiment selection, comparison, verification and regeneration tasks. */
export function experimentCommands(program: Command) {
  const compare = program.command("compare").description("Compare source revisions or algorithms");
  const measurement = (name: string, description: string) =>
    addSelectionOptions(compare.command(name))
      .description(description)
      .option("--out <directory>", "new result directory (default: bench/results/<timestamp>)")
      .option("--cpu <list>", "pin measurement to a CPU list or range (Linux)", parseLogicalCpu);

  measurement("commit [revision]", "Compare the working tree with a commit (default: HEAD)")
    .option("--filter <regex>", "additional case ID pattern", parseCasePattern)
    .addOption(
      new Option(
        "--layout-control <control>",
        "local build control; requires layout seeds",
      ).choices(LAYOUT_CONTROLS),
    )
    .option(
      "--layout-seeds <seeds>",
      "comma-separated fixed LLD layout seeds (Linux GNU)",
      (value) => {
        try {
          return parseLayoutSeeds(value);
        } catch (error) {
          throw new InvalidArgumentError((error as Error).message);
        }
      },
    )
    .option("--prepare-only", "retain a complete revision pair without timing")
    .option(
      "--shard <index/count>",
      "measure one balanced partition of the resolved cases",
      parseShard,
    )
    .action(
      async (
        baseline: string | undefined,
        options: MeasurementOptions & {
          shard?: { index: number; count: number };
          prepareOnly?: boolean;
          filter?: string;
          layoutSeeds?: number[];
          layoutControl?: LayoutControl;
        },
      ) => {
        if (options.layoutControl && !options.layoutSeeds)
          throw new InvalidArgumentError("layout controls require --layout-seeds");
        const plan = await loadPlan(options.plan ?? "local", selectAxes(options));
        if (plan.harness !== "smoke")
          throw new Error("Commit comparisons require a smoke harness plan");
        const root = await repositoryRoot();
        const out = resultDirectory(root, options.out);
        await runTask(out, Boolean(program.opts().verbose), () =>
          compareLocal({
            root,
            baseline: baseline ?? "HEAD",
            plan,
            out,
            cpu: options.cpu,
            filter: options.filter,
            layoutSeeds: options.layoutSeeds,
            layoutControl: options.layoutControl,
            shard: options.shard,
            prepareOnly: options.prepareOnly,
          }),
        );
      },
    );

  compare
    .command("prepared <directory>")
    .description("Measure and report a frozen revision pair without compiling")
    .option("--out <directory>", "new result directory")
    .option("--cpu <list>", "pin measurement to a CPU list", parseLogicalCpu)
    .option("--shard <index/count>", "measure one complete-case partition", parseShard)
    .action(
      async (
        directory: string,
        options: { out?: string; cpu?: string; shard?: { index: number; count: number } },
      ) => {
        const root = await repositoryRoot();
        const out = resultDirectory(root, options.out);
        await runTask(out, Boolean(program.opts().verbose), () =>
          comparePrepared({
            root,
            prepared: resolve(directory),
            out,
            cpu: options.cpu,
            shard: options.shard,
          }),
        );
      },
    );

  measurement(
    "counters [revision]",
    "Compare fixed-work serial instruction and simulated event counts",
  )
    .option(
      "--valgrind-image <image>",
      "run Valgrind in a prepared Docker image with a compatible libc",
    )
    .option(
      "--sorts <number>",
      "prepared sorts per collected batch (1..1024)",
      parsePositiveInteger,
      8,
    )
    .action(
      async (
        baseline: string | undefined,
        options: MeasurementOptions & { sorts: number; valgrindImage?: string },
      ) => {
        const plan = await loadPlan(options.plan ?? "local", selectAxes(options));
        if (plan.harness !== "smoke") throw new Error("Counters require a smoke harness plan");
        const root = await repositoryRoot();
        const dirty = await workingTreeDirty(root);
        const out = resultDirectory(root, options.out);
        await runTask(out, Boolean(program.opts().verbose), () =>
          compareCounters({
            root,
            out,
            baseline: baseline ?? "HEAD",
            plan,
            sorts: options.sorts,
            dirty,
            cpu: options.cpu,
            valgrindImage: options.valgrindImage,
          }),
        );
      },
    );

  measurement("algorithms", "Prepare, validate, measure and report a resumable comparison")
    .option(
      "--sample-ms <number>",
      "minimum timed sample duration in milliseconds",
      parsePositiveInteger,
      5,
    )
    .option(
      "--memory-mib <number>",
      "prepared-buffer cap in MiB (at least one input)",
      parsePositiveInteger,
      256,
    )
    .option("--group <type/shape>", "measure one complete group and report it as partial")
    .action(
      async (
        options: MeasurementOptions & { group?: string; sampleMs: number; memoryMib: number },
      ) => {
        const plan = await loadPlan(options.plan ?? "smoke", selectAxes(options));
        if (plan.harness !== "comparison")
          throw new Error("Algorithm comparisons require a comparison harness plan");
        const root = await repositoryRoot();
        const out = resultDirectory(root, options.out);
        await runTask(out, Boolean(program.opts().verbose), () =>
          compareAlgorithms({
            root,
            out,
            cpu: options.cpu,
            plan,
            group: options.group,
            sampleMs: options.sampleMs,
            memoryMib: options.memoryMib,
          }),
        );
      },
    );

  compare
    .command("resume <directory>")
    .description("Resume retained builds with the recorded selection and settings")
    .action(async (directory: string) => {
      const out = resolve(directory);
      const log = `${out}.resume-${Date.now()}-${process.pid}.log`;
      await runTask(out, Boolean(program.opts().verbose), () => resumeComparison(out), log);
    });

  addSelectionOptions(program.command("plan [name-or-file]"))
    .description("Print a resolved plan without compiling or measuring")
    .addOption(
      new Option("--format <format>", "output encoding").choices(["json", "toml"]).default("json"),
    )
    .option("--schema", "print the plan JSON Schema")
    .action(
      async (
        name: string | undefined,
        options: MeasurementOptions & { schema?: boolean; format: "json" | "toml" },
      ) => {
        if (options.schema) {
          console.log(JSON.stringify(planSchema()));
          return;
        }
        const plan = await loadPlan(options.plan ?? name ?? "local", selectAxes(options));
        console.log(
          options.format === "toml" ? stringifyToml(plan).trimEnd() : JSON.stringify(plan),
        );
      },
    );

  program
    .command("report <directories...>")
    .description("Regenerate figures and tables without building")
    .option("--partial", "allow completed groups of an algorithm suite")
    .action(async (directories: string[], options: { partial?: boolean }) => {
      const result = await report(directories, options.partial);
      console.log(JSON.stringify({ out: resolve(directories[0]), result: result ?? "reported" }));
    });

  program
    .command("check")
    .description("Reproduce CI's comparison harness checks, not a performance experiment")
    .option("--native", "include native adapters and verify sequential linkage (Linux)")
    .option("--out <directory>", "new result directory (default: bench/results/<timestamp>)")
    .action(async (options: { native?: boolean; out?: string }) => {
      const root = await repositoryRoot();
      const out = resultDirectory(root, options.out);
      await runTask(out, Boolean(program.opts().verbose), () =>
        checkAlgorithms(root, out, Boolean(options.native)),
      );
    });
}
