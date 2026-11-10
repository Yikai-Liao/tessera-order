// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { catalog, criterionId, parseCriterionId } from "../catalog.ts";
import { partition, runWorkers } from "./worker.ts";

const cli = fileURLToPath(new URL("../../tools/index.ts", import.meta.url));

test("trusted partition agrees with the ID owner at lane and mode boundaries", () => {
  // The catalog also names historical APIs; only the worker grants these phases.
  const modes = catalog.modes.filter((mode) => ["api", "par-4"].includes(mode.id));
  const lengths = [...new Set([...catalog.smoke.lengths, 999_999, 1_000_000, 1_000_001])];
  const cases = catalog.smoke.types.flatMap((ty) =>
    lengths.flatMap((len) =>
      modes.map((mode) => criterionId({ ty, distribution: "random", len }, mode.id)),
    ),
  );
  const expected: { small: string[][]; large: string[]; parallel: string[] } = {
    small: [[], []],
    large: [],
    parallel: [],
  };
  for (const id of cases.toSorted()) {
    const { len, mode } = parseCriterionId(id);
    if (mode === "par-4") expected.parallel.push(id);
    else if (len >= 1_000_000) expected.large.push(id);
    else {
      const lane = expected.small[0].length <= expected.small[1].length ? 0 : 1;
      expected.small[lane].push(id);
    }
  }
  assert.deepEqual(partition(cases), expected);
});

test("supervisor rejects invalid IDs before starting workers", {
  skip: process.platform !== "linux",
}, async (t) => {
  const root = await mkdtemp(join(tmpdir(), "tessera-scheduler-ids-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const id = "rust/u64/random/65536/s1/api";
  const lists = ["", `${id}\n${id}`, id.replace("/api", "/workspace")];
  for (const [index, listed] of lists.entries()) {
    const binary = join(root, `bench-${index}`);
    const started = join(root, `started-${index}`);
    const out = join(root, `results-${index}`);
    await writeFile(
      binary,
      [
        "#!/usr/bin/env node",
        "// SPDX-License-Identifier: BSD-2-Clause",
        `if (process.env.TESSERA_BENCH_LIST) console.log(${JSON.stringify(listed)});`,
        `else require('node:fs').writeFileSync(${JSON.stringify(started)}, 'started');`,
      ].join("\n"),
      { mode: 0o755 },
    );
    const result = spawnSync(process.execPath, [cli, "continuous", "run", binary, "--out", out], {
      encoding: "utf8",
      timeout: 5000,
    });
    assert.equal(result.status, 1, result.stderr);
    assert.match(
      await readFile(`${out}.log`, "utf8"),
      /Expected distinct benchmark IDs|Invalid benchmark ID/,
    );
    await assert.rejects(readFile(started), { code: "ENOENT" });
  }
});

test("failed workers are reaped and their siblings are killed", {
  skip: process.platform !== "linux",
}, async (t) => {
  const root = await mkdtemp(join(tmpdir(), "tessera-worker-failure-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const allowed = (await readFile("/proc/self/status", "utf8")).match(
    /^Cpus_allowed_list:\s*(.+)$/m,
  )![1];
  const cpu = Number(allowed.split(/[,-]/)[0]);
  const binary = join(root, "bench");
  await writeFile(
    binary,
    `#!/usr/bin/env node
// SPDX-License-Identifier: BSD-2-Clause
const fs = require("node:fs");
if (process.env.TESSERA_BENCH_GROUPS === "fail") setTimeout(() => process.exit(1), 200);
else {
  fs.writeFileSync(${JSON.stringify(join(root, "pid"))}, String(process.pid));
  setInterval(() => {}, 1000);
}
`,
    { mode: 0o755 },
  );
  await assert.rejects(
    runWorkers(binary, [["fail/api"], ["hang/api"]], [cpu, cpu], join(root, "workers")),
  );
  const pid = Number(await readFile(join(root, "pid"), "utf8"));
  assert.throws(() => process.kill(pid, 0), { code: "ESRCH" });
});
