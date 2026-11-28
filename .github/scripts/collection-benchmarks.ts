// SPDX-License-Identifier: BSD-2-Clause

import { appendFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { catalog, operationKey } from "../../bench/catalog.ts";
import { loadPlan, plansByOperation } from "../../bench/plan.ts";
import { command } from "../../tools/process.ts";

const [mode, preset, index] = process.argv.slice(2);
if (
  !["collections-smoke", "collections-pilot", "collections-decision", "collections-large"].includes(
    preset,
  )
)
  throw new Error("Choose a collection preset");

const shards = plansByOperation(await loadPlan(preset)).flatMap((plan) => {
  if (
    plan.include[0].operation?.kind !== "map-build-insert-random" ||
    preset !== "collections-decision"
  )
    return [plan];
  return plan.include.flatMap((rule) =>
    rule.types!.flatMap((type) =>
      rule.distributions!.map((distribution) => ({
        ...plan,
        include: [{ ...rule, types: [type], distributions: [distribution] }],
      })),
    ),
  );
});

if (mode === "matrix") {
  const matrix = JSON.stringify({ include: shards.map((_, shard) => ({ shard })) });
  if (process.env.GITHUB_OUTPUT)
    await appendFile(process.env.GITHUB_OUTPUT, "matrix=" + matrix + "\n");
  else console.log(matrix);
} else if (mode === "run") {
  const indices = [Number(index)];
  const status = await readFile("/proc/self/status", "utf8");
  const cpu = status.match(/^Cpus_allowed_list:\s*(\d+)/m)?.[1];
  if (!cpu) throw new Error("No allowed logical CPU");
  const output = join("bench/results/diagnostics", preset);
  await mkdir(output, { recursive: true });
  for (const i of indices) {
    const plan = shards[i];
    if (!plan) throw new Error("Unknown collection shard");
    const stem = i + "-" + operationKey(plan.include[0].operation!).replaceAll("/", "-");
    const path = join(output, stem + ".json");
    await writeFile(path, JSON.stringify(plan));
    const blocks =
      preset === "collections-decision" || preset === "collections-large"
        ? catalog.pairedBlocks.extended.count
        : catalog.pairedBlocks.short.count;
    await command("bun", [
      "run",
      "tools",
      "compare",
      "implementations",
      "--plan",
      path,
      "--cpu",
      cpu,
      "--blocks",
      String(blocks),
      "--sample-ms",
      preset === "collections-smoke" ? "1" : "5",
      "--memory-mib",
      "64",
      "--out",
      join(output, stem),
    ]);
  }
} else {
  throw new Error("Expected matrix or run");
}
