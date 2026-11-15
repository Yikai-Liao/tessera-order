// SPDX-License-Identifier: BSD-2-Clause

import { operationKinds } from "../catalog.ts";
import { type Plan, parsePlan } from "../plan.ts";

/** Select complete workload groups from the continuous plan. CI's path router
 * owns which groups a change affects; this operation owns their plan meaning.
 * Unknown or absent groups fail before builds, never become an empty success. */
export function selectContinuous(plan: Plan, selection: string): Plan {
  if (selection === "all") return plan;
  if (plan.operation) throw new Error("Group selection requires a mixed revision plan");
  const groups = selection.split(",");
  const valid = new Set<string>([...operationKinds, "parallel"]);
  if (new Set(groups).size !== groups.length || groups.some((group) => !valid.has(group)))
    throw new Error("Unknown or duplicate benchmark groups");
  const include = plan.include.filter((rule) => groups.includes(rule.operation?.kind ?? "sort"));
  for (const group of groups.filter((group) => group !== "parallel"))
    if (!include.some((rule) => (rule.operation?.kind ?? "sort") === group))
      throw new Error(`The plan has no benchmark group: ${group}`);
  const parallel = groups.includes("parallel") ? plan.parallel : undefined;
  if (groups.includes("parallel") && !parallel?.length)
    throw new Error("The plan has no parallel benchmark group");
  // A parallel-only plan uses its selected inputs as the ordinary include
  // rules, avoiding unused serial rules and their operation build features.
  return parsePlan({
    schema: 1,
    algorithms: [...(include.length ? ["tessera"] : []), ...(parallel ? ["tessera-par-4"] : [])],
    include: include.length ? include : parallel,
    exclude: plan.exclude,
    ...(include.length && parallel ? { parallel } : {}),
  });
}
