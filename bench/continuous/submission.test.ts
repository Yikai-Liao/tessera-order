// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { VARIANTS } from "../catalog.ts";
import { writePaired } from "./fixtures/paired.ts";
import { baseHash, fixture, hash } from "./fixtures/report.ts";
import { pairedBmf } from "./paired.ts";

test("submission archives ratios on the PR branch without resetting its Head", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-submission-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const runs = await writePaired(directory, VARIANTS[0]);
  const report = fixture();
  report.testbed.name = VARIANTS[0].relativeTestbed;
  report.branch.name = "pr-7";
  report.branch.head.start_point = null;
  report.results = [
    Object.entries(pairedBmf(runs)).map(([name, values]) => ({
      benchmark: { name, uuid: name },
      measures: [
        {
          measure: { name: "paired-time-ratio", uuid: "ratio" },
          metric: { uuid: name, value: values["paired-time-ratio"].value },
        },
      ],
    })),
  ];
  await writeFile(join(directory, "cloud.json"), JSON.stringify(report));
  const bin = join(directory, "bin");
  await mkdir(bin);
  await writeFile(
    join(bin, "bencher"),
    `#!/usr/bin/env node
// SPDX-License-Identifier: BSD-2-Clause
const fs = require("node:fs");
const path = require("node:path");
fs.writeFileSync(path.join(process.env.SUBMISSION_FIXTURE, "args.json"), JSON.stringify(process.argv.slice(2)));
process.stdout.write(fs.readFileSync(path.join(process.env.SUBMISSION_FIXTURE, "cloud.json")));
`,
    { mode: 0o755 },
  );
  const options = {
    artifacts: directory,
    prepared: join(directory, "prepared"),
    project: "project",
    out: join(directory, "submitted"),
    hash,
    baseHash,
    pr: "7",
  };
  const source = `import {submitContinuous} from ${JSON.stringify(new URL("./runner.ts", import.meta.url).href)}; const result=await submitContinuous(${JSON.stringify(options)}); if(Object.values(result).includes("failure")) process.exitCode=1;`;
  const child = spawnSync(process.execPath, ["--input-type=module", "-e", source], {
    encoding: "utf8",
    env: {
      ...process.env,
      PATH: `${bin}:${process.env.PATH}`,
      SUBMISSION_FIXTURE: directory,
      BENCHER_API_KEY: "fixture-key",
    },
  });
  assert.equal(child.status, 0, child.stderr);
  const args = JSON.parse(await readFile(join(directory, "args.json"), "utf8")) as string[];
  assert.equal(args[args.indexOf("--branch") + 1], "pr-7");
  assert.ok(!args.some((arg) => arg.startsWith("--start-point")));
});
