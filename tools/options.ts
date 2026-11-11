// SPDX-License-Identifier: BSD-2-Clause

import { InvalidArgumentError } from "commander";
import { cpuListSchema } from "../bench/affinity.ts";

/** Validate explicit Linux affinity before a task creates outputs or builds. */
export function parseLogicalCpu(value: string) {
  if (!cpuListSchema.safeParse(value).success || process.platform !== "linux")
    throw new InvalidArgumentError("expected a Linux CPU list, such as 2 or 0,2,4,6");
  return value;
}

/** Decode a complete-case partition; the runner owns inventory assignment. */
export function parseShard(value: string) {
  const match = /^([1-9]\d*)\/([1-9]\d*)$/.exec(value);
  if (!match || Number(match[1]) > Number(match[2]) || Number(match[2]) > 64)
    throw new InvalidArgumentError("expected index/count with 1 <= index <= count <= 64");
  return { index: Number(match[1]), count: Number(match[2]) };
}
