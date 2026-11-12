// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

test("Bun documentation checks reject script, template and component-prop errors", async (t) => {
  const target = fileURLToPath(new URL("../../target/", import.meta.url));
  await mkdir(target, { recursive: true });
  const directory = await mkdtemp(join(target, "docs-typecheck-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const config = join(directory, "tsconfig.json");
  await writeFile(
    config,
    JSON.stringify({
      extends: fileURLToPath(new URL("./tsconfig.json", import.meta.url)),
      include: ["*.ts", "*.vue"],
    }),
  );
  await writeFile(
    join(directory, "Child.vue"),
    `<script setup lang="ts">
defineProps<{ count: number }>();
</script>
<template><span>{{ count }}</span></template>
`,
  );
  const source = (invalid: boolean) => `<script setup lang="ts">
import Child from "./Child.vue";
const count: number = ${invalid ? '"wrong"' : "1"};
</script>
<template>
  <Child :count="${invalid ? "'wrong'" : "count"}" />
  <span>{{ count.${invalid ? "toUpperCase" : "toFixed"}() }}</span>
</template>
`;
  const check = () =>
    spawnSync(
      process.execPath,
      [
        fileURLToPath(new URL("./typecheck.ts", import.meta.url)),
        "--noEmit",
        "--pretty",
        "false",
        "-p",
        config,
      ],
      { encoding: "utf8", timeout: 20000 },
    );
  const component = join(directory, "Parent.vue");
  await writeFile(component, source(false));
  let result = check();
  assert.equal(result.status, 0, result.stdout + result.stderr);
  await writeFile(component, source(true));
  result = check();
  assert.equal(result.status, 2, result.stdout + result.stderr);
  assert.match(result.stdout, /Parent.vue\(3,7\): error TS2322/);
  assert.match(result.stdout, /Parent.vue\(6,11\): error TS2322/);
  assert.match(result.stdout, /Parent.vue\(7,18\): error TS2339/);
});
