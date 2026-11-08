// SPDX-License-Identifier: BSD-2-Clause

import { access, mkdir } from "node:fs/promises";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { Command, InvalidArgumentError, Option } from "commander";
import { stringify as stringifyToml } from "smol-toml";
import { renderLocalReport } from "./local/report.ts";
import { compareLocal } from "./local/runner.ts";
import { loadPlan, planSchema, type SelectionOptions } from "./plan.ts";
import { pullFeedback } from "./pr.ts";
import { command } from "./process.ts";
import { compareAlgorithms } from "./release.ts";
import { writeReport } from "./report/index.ts";

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
    .option("--lengths <numbers>", "comma-separated supported record counts", lengths)
    .option("--modes <ids>", "comma-separated API modes", list);
}

function selectors(options: MeasurementOptions): SelectionOptions {
  return Object.fromEntries(
    ["algorithms", "types", "distributions", "lengths", "modes"]
      .filter((key) => options[key as keyof MeasurementOptions] !== undefined)
      .map((key) => [key, options[key as keyof MeasurementOptions]]),
  );
}

async function repository() {
  return command("git", ["rev-parse", "--show-toplevel"], { capture: true });
}

async function resultDirectory(root: string, out?: string) {
  const directory = resolve(
    out ?? join(root, "bench/results", new Date().toISOString().replaceAll(":", "-")),
  );
  await mkdir(resolve(directory, ".."), { recursive: true });
  console.log(`Results: ${directory}`);
  return directory;
}

/** Regenerate a report from its persisted result format. */
export async function report(directories: string[], partial = false) {
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

/** Construct the supported command surface without running an experiment. */
export function createProgram() {
  const program = new Command()
    .name("pnpm bench")
    .description("Local comparisons, release figures and continuous benchmark feedback")
    .showHelpAfterError()
    .exitOverride();

  const measurement = (name: string, description: string) =>
    selection(program.command(name))
      .description(description)
      .option("--out <directory>", "result directory (default: bench/results/<timestamp>)")
      .option("--cpu <number>", "pin measurement to one logical CPU (Linux)", logicalCpu)
      .option("--simd", "include Portable SIMD");

  measurement("compare [commit]", "Compare the working tree with a commit (default: HEAD)")
    .option("--filter <regex>", "additional case ID pattern", casePattern)
    .action(
      async (baseline: string | undefined, options: MeasurementOptions & { filter?: string }) => {
        const plan = await loadPlan(options.plan ?? "local", selectors(options));
        if (plan.harness !== "smoke")
          throw new Error("Commit comparisons require a smoke harness plan");
        const root = await repository();
        await compareLocal({
          root,
          baseline: baseline ?? "HEAD",
          plan,
          out: await resultDirectory(root, options.out),
          cpu: options.cpu,
          simd: Boolean(options.simd),
          filter: options.filter,
        });
      },
    );

  measurement("run", "Build, validate, measure and report algorithm comparisons")
    .alias("release")
    .option("--group <type/shape>", "measure one complete group and report it as partial")
    .action(async (options: MeasurementOptions & { group?: string }) => {
      const plan = await loadPlan(options.plan ?? "smoke", selectors(options));
      if (plan.harness !== "comparison")
        throw new Error("Algorithm comparisons require a comparison harness plan");
      const root = await repository();
      await compareAlgorithms({
        root,
        out: await resultDirectory(root, options.out),
        cpu: options.cpu,
        simd: Boolean(options.simd),
        plan,
        group: options.group,
      });
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
          console.log(JSON.stringify(planSchema(), null, 2));
          return;
        }
        const plan = await loadPlan(options.plan ?? name ?? "local", selectors(options));
        console.log(
          options.format === "toml" ? stringifyToml(plan).trimEnd() : JSON.stringify(plan, null, 2),
        );
      },
    );

  program
    .command("report <directories...>")
    .description("Regenerate figures and tables without building")
    .option("--partial", "allow completed groups of an algorithm suite")
    .action(async (directories: string[], options: { partial?: boolean }) => {
      await report(directories, options.partial);
    });

  program
    .command("pr <number>")
    .description("Read current-head continuous benchmark checks and report")
    .option("--json", "emit machine-readable feedback")
    .action(async (number: string, options: { json?: boolean }) => {
      const feedback = await pullFeedback(number, await repository());
      if (options.json) {
        console.log(JSON.stringify(feedback, null, 2));
        return;
      }
      console.log(`PR #${feedback.number} · ${feedback.head}\n${feedback.url}`);
      for (const check of feedback.checks) {
        console.log(
          `\n${check.name}: ${check.conclusion ?? check.status}\n${check.output.title ?? ""}\n${check.details_url}\n${check.output.summary ?? ""}`,
        );
      }
      console.log(
        feedback.comment
          ? `\n${feedback.comment.url}\n${feedback.comment.body}`
          : "\nNo current-head benchmark comment yet.",
      );
    });

  return program;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  createProgram()
    .parseAsync()
    .catch((error: unknown) => {
      if (
        error instanceof Error &&
        "code" in error &&
        ["commander.helpDisplayed", "commander.help"].includes(String(error.code))
      )
        return;
      if (
        !(error instanceof Error && "code" in error && String(error.code).startsWith("commander."))
      )
        console.error(error instanceof Error ? error.message : String(error));
      process.exitCode = 1;
    });
}
