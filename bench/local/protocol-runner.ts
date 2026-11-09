// SPDX-License-Identifier: BSD-2-Clause

import { mkdir, writeFile } from "node:fs/promises";
import { arch, cpus, platform, release } from "node:os";
import { join } from "node:path";
import { command, diagnostic } from "../../tools/process.ts";
import type { Plan } from "../plan.ts";
import { withWorkspace } from "../workspace.ts";
import {
  type ExpectedDirection,
  PROTOCOL_LAYOUTS,
  PROTOCOL_TRIALS,
  type ProtocolRun,
  protocolSeeds,
  type StudyTiming,
  validateProtocolBuilds,
  validateProtocolSettings,
} from "./protocol.ts";
import { renderProtocolReport } from "./protocol-report.ts";
import {
  assertWorkerEnvironment,
  buildRevisions,
  measureBlock,
  readLayoutLinker,
  selectedCases,
} from "./runner.ts";

interface ProtocolOptions {
  root: string;
  baseline: string;
  out: string;
  plan: Plan;
  simd: boolean;
  cpu?: string;
  firstSeed: number;
  expected: ExpectedDirection;
  control?: string;
}

/** Ask the measured executable for its effective case settings without generating inputs. */
async function readTimings(binary: string, planPath: string, divisor: number, cwd: string) {
  const output = await command(binary, ["--bench"], {
    cwd,
    capture: true,
    env: {
      TESSERA_BENCH_PLAN: planPath,
      TESSERA_BENCH_STUDY_SETTINGS: "1",
      TESSERA_BENCH_TIME_DIVISOR: String(divisor),
    },
  });
  const timings: Record<string, StudyTiming> = {};
  for (const line of output.split("\n").filter(Boolean)) {
    const { id, ...timing } = JSON.parse(line);
    if (typeof id !== "string" || Object.hasOwn(timings, id))
      throw new Error("Invalid or duplicate timing case");
    timings[id] = timing;
  }
  return timings;
}

/** Build all held-out layouts, then compare full and divided budgets in paired trials. */
export async function compareProtocols(options: ProtocolOptions) {
  const { root, out, cpu } = options;
  assertWorkerEnvironment();
  const seeds = protocolSeeds(options.firstSeed);
  const linker = await readLayoutLinker();
  const baseline = await command("git", ["rev-parse", "--verify", `${options.baseline}^{commit}`], {
    cwd: root,
    capture: true,
  });
  const head = await command("git", ["rev-parse", "HEAD"], { cwd: root, capture: true });
  const dirty = Boolean(
    await command("git", ["status", "--porcelain"], { cwd: root, capture: true }),
  );
  await mkdir(out, { recursive: false });
  const planPath = join(out, "plan.json");
  await writeFile(planPath, `${JSON.stringify(options.plan, null, 2)}\n`);
  await withWorkspace(root, async (workspace) => {
    const binaries: { baseline: string; current: string }[][] = [];
    let run: ProtocolRun | undefined;
    for (let trial = 0; trial < PROTOCOL_TRIALS; trial++) {
      binaries.push([]);
      const pairs: ProtocolRun["trials"][number]["pairs"] = [];
      for (let pair = 0; pair < PROTOCOL_LAYOUTS; pair++) {
        const seedIndex = (trial * PROTOCOL_LAYOUTS + pair) * 2;
        const [baselineSeed, currentSeed] =
          (trial + pair) % 2
            ? [seeds[seedIndex + 1], seeds[seedIndex]]
            : [seeds[seedIndex], seeds[seedIndex + 1]];
        diagnostic(
          `Building trial ${trial + 1}/9, layout pair ${pair + 1}/3: ${baselineSeed}/${currentSeed}`,
        );
        const built = await buildRevisions(
          root,
          workspace,
          baseline,
          [...(options.simd ? ["portable-simd"] : []), "layout-study"],
          { baselineSeed, currentSeed, out },
        );
        const cases = await selectedCases(built.binaries, planPath, undefined, workspace.tree);
        if (!run) {
          run = {
            schema: 3,
            protocol: "candidate-only-vs-paired-median3-v1",
            expected: options.expected,
            controls: options.control
              ? cases.filter((id) => new RegExp(options.control!).test(id))
              : [],
            linker,
            provenance: {
              baseline,
              head,
              dirty,
              source: built.source,
              rustc: built.builds.current.rustc,
              flags: built.builds.current.cargo_encoded_rustflags,
              configuration: options.simd ? "portable-simd" : "default",
              host: {
                platform: platform(),
                release: release(),
                arch: arch(),
                cpu: cpus()[0]?.model ?? "unknown",
                affinity: cpu ?? "inherited",
              },
              cases,
            },
            trials: [],
          };
          if ((options.expected === "same") !== (built.source.baseline === built.source.current))
            throw new Error("Source hashes do not match the declared experiment expectation");
          if (options.control && !run.controls.length)
            throw new Error("Control pattern must select at least one timed case");
        } else if (
          JSON.stringify(built.source) !== JSON.stringify(run.provenance.source) ||
          cases.join("\n") !== run.provenance.cases.join("\n")
        )
          throw new Error("Protocol layouts must share sources and selected cases");
        pairs.push({ baselineSeed, currentSeed, binaries: built.hashes, builds: built.builds });
        binaries[trial].push(built.binaries);
      }
      run!.trials.push({ pairs, singlePair: trial % PROTOCOL_LAYOUTS, multiple: [], wallMs: {} });
      await writeFile(join(out, "local.json"), `${JSON.stringify(run, null, 2)}\n`);
    }
    if (!run) throw new Error("Missing protocol builds");
    validateProtocolBuilds(run);
    const settings = async (binary: string) => ({
      single: await readTimings(binary, planPath, 1, workspace.tree),
      multiple: await readTimings(binary, planPath, 6, workspace.tree),
    });
    run.timings = await settings(binaries[0][0].baseline);
    if (JSON.stringify(run.timings) !== JSON.stringify(await settings(binaries[0][0].current)))
      throw new Error("Effective timing settings differ between revisions");
    validateProtocolSettings(run);
    const save = () => writeFile(join(out, "local.json"), `${JSON.stringify(run, null, 2)}\n`);
    await save();
    for (const [trialIndex, trial] of run.trials.entries()) {
      const strategies =
        trialIndex % 2 ? (["multiple", "single"] as const) : (["single", "multiple"] as const);
      for (const strategy of strategies) {
        const started = performance.now();
        diagnostic(`Trial ${trialIndex + 1}/9: ${strategy}`);
        const pairIndices =
          strategy === "single"
            ? [trial.singlePair]
            : Array.from(
                { length: PROTOCOL_LAYOUTS },
                (_, index) => (index + trialIndex) % PROTOCOL_LAYOUTS,
              );
        for (const pairIndex of pairIndices) {
          const block = await measureBlock(
            run.provenance.cases,
            binaries[trialIndex][pairIndex],
            workspace,
            { block: trialIndex, planPath, cpu, timeDivisor: strategy === "single" ? 1 : 6 },
          );
          if (strategy === "single") trial.single = block;
          else trial.multiple[pairIndex] = block;
          await save();
        }
        trial.wallMs[strategy] = performance.now() - started;
        await save();
      }
    }
    await renderProtocolReport(out, run);
  });
}
