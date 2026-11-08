// SPDX-License-Identifier: BSD-2-Clause

import { access } from "node:fs/promises";
import { join, resolve } from "node:path";
import { type Command, InvalidArgumentError, Option } from "commander";
import { stringify as stringifyToml } from "smol-toml";
import { renderLocalReport } from "../bench/local/report.ts";
import { compareLocal } from "../bench/local/runner.ts";
import { loadPlan, planSchema, type SelectionOptions } from "../bench/plan.ts";
import { checkAlgorithms, compareAlgorithms } from "../bench/release.ts";
import { writeReport } from "../bench/report/index.ts";
import { repositoryRoot } from "../bench/workspace.ts";
import { resultDirectory, runTask } from "./tasks.ts";

interface MeasurementOptions extends SelectionOptions {
  plan?: string;
  out?: string;
  cpu?: string;
  simd?: boolean;
}

function logicalCpu(value: string) {
  if (!/^\d+$/.test(value) || process.platform !== "linux")
    throw new InvalidArgumentError("expected one logical CPU number on Linux");
  return value;
}

function casePattern(value: string) {
  try {
    new RegExp(value);
  } catch {
    throw new InvalidArgumentError("expected a valid case ID regular expression");
  }
  return value;
}

function list(value: string) {
  const values = value.split(",").map((item) => item.trim());
  if (values.some((item) => !item))
    throw new InvalidArgumentError("expected comma-separated values");
  return values;
}

function lengths(value: string) {
  const values = list(value).map(Number);
  if (values.some((item) => !Number.isSafeInteger(item) || item < 1))
    throw new InvalidArgumentError("expected positive integer lengths");
  return values;
}

function selection(command: Command) {
  return command
    .option("--plan <name-or-file>", "JSON/TOML plan or local/continuous/smoke/release preset")
    .option(
      "--algorithms <ids>",
      "comma-separated algorithms (required controls are retained)",
      list,
    )
    .option("--types <ids>", "comma-separated record types", list)
    .option("--distributions <ids>", "comma-separated input distributions", list)
    .option("--lengths <numbers>", "comma-separated positive record counts", lengths)
    .option("--modes <ids>", "comma-separated API modes", list);
}

function selectors(options: MeasurementOptions): SelectionOptions {
  return Object.fromEntries(
    ["algorithms", "types", "distributions", "lengths", "modes"]
      .filter((key) => options[key as keyof MeasurementOptions] !== undefined)
      .map((key) => [key, options[key as keyof MeasurementOptions]]),
  );
}

/** Regenerate a report from its persisted result format without rerunning measurements. */
async function report(directories: string[], partial = false) {
  const paths = directories.map((path) => resolve(path));
  let local = false;
  try {
    await access(join(paths[0], "local.json"));
    local = true;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
  }
  if (local) {
    if (paths.length !== 1 || partial)
      throw new Error("Local reports require one directory and nine complete blocks");
    await renderLocalReport(paths[0]);
  } else await writeReport(paths, partial);
}

/** Register experiment selection, comparison, verification and regeneration tasks. */
export function experimentCommands(program: Command) {
  const compare = program.command("compare").description("Compare source revisions or algorithms");
  const measurement = (name: string, description: string) =>
    selection(compare.command(name))
      .description(description)
      .option("--out <directory>", "new result directory (default: bench/results/<timestamp>)")
      .option("--cpu <number>", "pin measurement to one logical CPU (Linux)", logicalCpu)
      .option("--simd", "include Portable SIMD");

  measurement("commit [revision]", "Compare the working tree with a commit (default: HEAD)")
    .option("--filter <regex>", "additional case ID pattern", casePattern)
    .action(
      async (baseline: string | undefined, options: MeasurementOptions & { filter?: string }) => {
        const plan = await loadPlan(options.plan ?? "local", selectors(options));
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
            simd: Boolean(options.simd),
            filter: options.filter,
          }),
        );
      },
    );

  measurement("algorithms", "Build, validate, measure and report algorithm comparisons")
    .option("--group <type/shape>", "measure one complete group and report it as partial")
    .action(async (options: MeasurementOptions & { group?: string }) => {
      const plan = await loadPlan(options.plan ?? "smoke", selectors(options));
      if (plan.harness !== "comparison")
        throw new Error("Algorithm comparisons require a comparison harness plan");
      const root = await repositoryRoot();
      const out = resultDirectory(root, options.out);
      await runTask(out, Boolean(program.opts().verbose), () =>
        compareAlgorithms({
          root,
          out,
          cpu: options.cpu,
          simd: Boolean(options.simd),
          plan,
          group: options.group,
        }),
      );
    });

  selection(program.command("plan [name-or-file]"))
    .description("Print a resolved plan without compiling or measuring")
    .addOption(
      new Option("--format <format>", "output encoding").choices(["json", "toml"]).default("toml"),
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
        const plan = await loadPlan(options.plan ?? name ?? "local", selectors(options));
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
      await report(directories, options.partial);
      console.log(JSON.stringify({ out: resolve(directories[0]), result: "reported" }));
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
