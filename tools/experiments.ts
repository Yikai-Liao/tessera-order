// SPDX-License-Identifier: BSD-2-Clause

import { access } from "node:fs/promises";
import { join, resolve } from "node:path";
import { type Command, InvalidArgumentError, Option } from "commander";
import { stringify as stringifyToml } from "smol-toml";
import { catalog, operationKinds, parseRankRange } from "../bench/catalog.ts";
import { compareCounters, probeCounters, reportCounters } from "../bench/local/counters.ts";
import { LAYOUT_CONTROLS, type LayoutControl, parseLayoutSeeds } from "../bench/local/layout.ts";
import { renderLocalReport } from "../bench/local/report.ts";
import { compareLocal } from "../bench/local/runner.ts";
import { loadPlan, planSchema, type SelectionOptions } from "../bench/plan.ts";
import { compareAlgorithms, resumeComparison } from "../bench/release.ts";
import { writeReport } from "../bench/report/index.ts";
import { pairedBudget } from "../bench/statistics.ts";
import { workingTreeDirty } from "../bench/workspace.ts";
import { parseLogicalCpu } from "./options.ts";
import { repositoryRoot, resultDirectory, runTask } from "./tasks.ts";

interface MeasurementOptions extends SelectionOptions {
  plan?: string;
  out?: string;
  cpu?: string;
}

function parseCount(value: string) {
  const number = Number(value);
  if (!/^(0|[1-9]\d*)$/.test(value) || !Number.isSafeInteger(number))
    throw new InvalidArgumentError("expected a nonnegative integer");
  return number;
}

function parsePositiveInteger(value: string) {
  const number = Number(value);
  if (!Number.isSafeInteger(number) || number < 1)
    throw new InvalidArgumentError("expected a positive integer");
  return number;
}

function blockOption() {
  return new Option("--blocks <number>", "paired blocks per case (3: short screen; 9: extended)")
    .argParser((value) => {
      try {
        return pairedBudget(Number(value)).count;
      } catch (error) {
        throw new InvalidArgumentError((error as Error).message);
      }
    })
    .default(catalog.pairedBlocks.short.count);
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

function addSelectionOptions(command: Command, presets: string) {
  return command
    .option("--plan <name-or-file>", `JSON/TOML plan or ${presets} preset`)
    .option("--types <ids>", "comma-separated record types", parseList)
    .option("--distributions <ids>", "comma-separated input distributions", parseList)
    .option("--lengths <numbers>", "comma-separated positive record counts", parseLengths)
    .addOption(
      new Option("--operation <kind>", "result contract (default: sort)").choices(operationKinds),
    )
    .option("--k <count>", "number of smallest records for unordered top-k", parseCount)
    .option("--prefix-len <count>", "required sorted prefix length", parseCount)
    .option("--index <rank>", "zero-based selection rank", parseCount)
    .option("--indices <ranks>", "comma-separated selection ranks, or none", (value) =>
      value === "none" ? [] : parseList(value).map(parseCount),
    )
    .option(
      "--segment-len <count>",
      "records per segment; last may be shorter",
      parsePositiveInteger,
    )
    .option("--rank-range <start:end>", "exact half-open ordered rank interval", (value) => {
      try {
        return parseRankRange(value);
      } catch (error) {
        throw new InvalidArgumentError((error as Error).message);
      }
    });
}

function selectAxes(options: MeasurementOptions): SelectionOptions {
  return Object.fromEntries(
    [
      "algorithms",
      "types",
      "distributions",
      "lengths",
      "operation",
      "prefixLen",
      "k",
      "index",
      "indices",
      "rankRange",
      "segmentLen",
    ]
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
        "Revision reports require one directory and a complete recorded budget per layout",
      );
    return renderLocalReport(paths[0]);
  }
  return writeReport(paths, partial);
}

/** Register experiment selection, comparison, verification and regeneration tasks. */
export function experimentCommands(program: Command) {
  const compare = program.command("compare").description("Compare source revisions or algorithms");
  const measurement = (name: string, description: string, presets: string) =>
    addSelectionOptions(compare.command(name), presets)
      .description(description)
      .option("--out <directory>", "new result directory (default: bench/results/<timestamp>)")
      .option("--cpu <list>", "pin measurement to a CPU list or range (Linux)", parseLogicalCpu);

  measurement(
    "commit [revision]",
    "Compare the working tree with a commit (default: HEAD)",
    "local/continuous",
  )
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
    .action(
      async (
        baseline: string | undefined,
        options: MeasurementOptions & {
          filter?: string;
          layoutSeeds?: number[];
          layoutControl?: LayoutControl;
        },
      ) => {
        if (options.layoutControl && !options.layoutSeeds)
          throw new InvalidArgumentError("layout controls require --layout-seeds");
        const plan = await loadPlan(options.plan ?? "local", selectAxes(options));
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
          }),
        );
      },
    );

  compare
    .command("probe-counters")
    .description("Probe Linux PMU events; never changes permissions")
    .requiredOption("--out <directory>", "new probe directory")
    .action(async (options: { out: string }) => {
      const out = resolve(options.out);
      await runTask(out, Boolean(program.opts().verbose), () => probeCounters(out));
    });

  measurement(
    "counters [revision]",
    "Compare fixed-work serial instruction and simulated event counts",
    "local/continuous",
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

  measurement(
    "algorithms",
    "Build, measure and report a resumable comparison",
    "smoke/release/parallel",
  )
    .option(
      "--algorithms <ids>",
      "comma-separated algorithms (required controls are retained)",
      parseList,
    )
    .addOption(blockOption())
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
        options: MeasurementOptions & {
          group?: string;
          sampleMs: number;
          memoryMib: number;
          blocks: number;
        },
      ) => {
        const plan = await loadPlan(options.plan ?? "smoke", selectAxes(options));
        const root = await repositoryRoot();
        const out = resultDirectory(root, options.out);
        await runTask(out, Boolean(program.opts().verbose), () =>
          compareAlgorithms({
            root,
            out,
            cpu: options.cpu,
            plan,
            group: options.group,
            blocks: options.blocks,
            sampleMs: options.sampleMs,
            memoryMib: options.memoryMib,
          }),
        );
      },
    );

  measurement(
    "revisions [baseline]",
    "Compare revisions and independent linked copies in one process",
    "smoke/release/parallel",
  )
    .addOption(blockOption())
    .option("--current <revision>", "current library commit (default: working snapshot)")
    .option(
      "--sample-ms <number>",
      "minimum timed sample duration in milliseconds",
      parsePositiveInteger,
      5,
    )
    .option("--memory-mib <number>", "prepared-buffer cap in MiB", parsePositiveInteger, 256)
    .action(
      async (
        baseline: string | undefined,
        options: MeasurementOptions & {
          current?: string;
          sampleMs: number;
          memoryMib: number;
          blocks: number;
        },
      ) => {
        const plan = await loadPlan(options.plan ?? "smoke", {
          ...selectAxes(options),
          algorithms: [
            "tessera",
            "std-stable",
            ...catalog.algorithms.filter((a) => a.adapter === "revision").map((a) => a.id),
          ],
        });
        const root = await repositoryRoot();
        const out = resultDirectory(root, options.out);
        await runTask(out, Boolean(program.opts().verbose), () =>
          compareAlgorithms({
            root,
            out,
            plan,
            cpu: options.cpu,
            blocks: options.blocks,
            sampleMs: options.sampleMs,
            memoryMib: options.memoryMib,
            revisions: { baseline: baseline ?? "HEAD", current: options.current },
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

  addSelectionOptions(
    program.command("plan [name-or-file]"),
    "local/continuous/main/smoke/release/parallel",
  )
    .option("--algorithms <ids>", "comma-separated algorithms", parseList)
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
}
