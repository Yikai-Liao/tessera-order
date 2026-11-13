// SPDX-License-Identifier: BSD-2-Clause

import { parseCriterionId, revisionSampling } from "../catalog.ts";

/** Bencher's remote limit for one job, in seconds; `bencher run` receives the same value. */
export const JOB_TIMEOUT_S = 300;

/**
 * Catalog warm-up and sampling time planned per remote job, in seconds.
 *
 * The catalog budget is a floor, not a duration: VM start, input preparation,
 * verification and Criterion analysis come on top, and a slow algorithm's
 * flat sampling can exceed its window. Remotely, `std-stable`'s 62 cases
 * (148 s of budget) took 185 s, and Tessera's 34 four-thread cases (164 s)
 * took 234 s; half the timeout keeps such jobs near 200 s.
 */
const JOB_SAMPLING_S = 0.5 * JOB_TIMEOUT_S;

/** A case's catalog warm-up and sampling budget, in seconds. */
function samplingSeconds(id: string) {
  const sampling = revisionSampling(parseCriterionId(id).len);
  return (sampling.warm_ms + sampling.window_ms) / 1000;
}

/**
 * Split one algorithm's inventory into the fewest remote jobs whose catalog
 * budget fits `JOB_SAMPLING_S`, keeping list order and balancing the jobs.
 * Each job is a complete Bencher run; the jobs together measure every case once.
 */
export function remoteJobs(ids: string[]): string[][] {
  const costs = ids.map(samplingSeconds);
  const total = costs.reduce((sum, cost) => sum + cost, 0);
  const count = Math.max(1, Math.ceil(total / JOB_SAMPLING_S));
  const jobs: string[][] = Array.from({ length: count }, () => []);
  let before = 0;
  for (const [index, id] of ids.entries()) {
    // Assign each case by the midpoint of its budget, so every job holds about total/count.
    const job = Math.min(count - 1, Math.floor(((before + costs[index] / 2) / total) * count));
    jobs[job].push(id);
    before += costs[index];
  }
  return jobs.filter((job) => job.length);
}
