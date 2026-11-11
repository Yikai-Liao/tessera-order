// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { cp, mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { command } from "../../../tools/process.ts";

test("native objects survive snapshot changes and invalidate on build inputs", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-native-cache-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const vendor = join(directory, "vendor");
  const out = join(directory, "out");
  await symlink(
    fileURLToPath(new URL("../../../node_modules", import.meta.url)),
    join(directory, "node_modules"),
    "dir",
  );
  await mkdir(out);
  // Tiny translation units exercise the builder without fetching upstream code.
  for (const source of [
    "clausecker/radixsort_permuted.c",
    ...["vqsort", "vqsort_u32a", "vqsort_u64a"].map(
      (name) => `highway/hwy/contrib/sort/${name}.cc`,
    ),
    ...[
      "x86simdsort",
      "x86simdsort-avx2",
      "x86simdsort-skx",
      "x86simdsort-icl",
      "x86simdsort-spr",
    ].map((name) => `x86simdsort/lib/${name}.cpp`),
  ]) {
    const path = join(vendor, source);
    await mkdir(join(path, ".."), { recursive: true });
    await writeFile(path, "fixture");
  }
  const first = join(directory, "first");
  const second = join(directory, "second");
  const native = join(first, "bench/rust/native");
  await mkdir(native, { recursive: true });
  await mkdir(join(first, "tools"));
  for (const name of ["build.ts", "sources.ts"])
    await cp(new URL(`./${name}`, import.meta.url), join(native, name));
  await writeFile(join(native, "sources.toml"), "[sources]\n");
  for (const name of [
    "layout.h",
    "registry.cpp",
    "simd.cpp",
    "ips.cpp",
    "duckdb.cpp",
    "fluxsort.c",
    "crumsort.c",
    "clausecker.c",
    "memory.cpp",
  ])
    await writeFile(join(native, name), "fixture");
  // Replace only the external toolchain; run the real cache and source reader.
  await writeFile(
    join(first, "tools/process.ts"),
    `
    import { appendFile, mkdir, readFile, writeFile } from "node:fs/promises";
    import { join, resolve } from "node:path";
    export async function command(program, args, options) {
      if (args[0] === "--version") return process.env.TEST_COMPILER_VERSION;
      if (program === "cmake" && args[0] === "-S") {
        await mkdir(args[3], { recursive: true });
        await writeFile(join(args[3], "source"), args[1]);
        await writeFile(join(args[3], "libhwy.a"), "archive");
      }
      if (args.includes("-c")) {
        const source = args[args.indexOf("-c") + 1];
        const object = args[args.indexOf("-o") + 1];
        await appendFile(join(process.env.TEST_NATIVE_OUT, "compiled"), source + "\\n");
        await writeFile(object, await readFile(resolve(options.cwd, source)));
      }
      return "";
    }
  `,
  );
  await cp(first, second, { recursive: true });
  for (const tree of [first, second]) await symlink(vendor, join(tree, "bench/vendor"), "dir");
  const build = (tree: string, version = "fixture compiler 1") =>
    command(process.execPath, [join(tree, "bench/rust/native/build.ts"), out], {
      capture: true,
      env: {
        TEST_NATIVE_OUT: out,
        TEST_COMPILER_VERSION: version,
        CC: "./fixture-cc",
        CXX: "./fixture-cxx",
        CFLAGS: "",
        CXXFLAGS: "",
        LDFLAGS: "",
        CARGO_BUILD_JOBS: "2",
      },
    });
  const compiled = () => readFile(join(out, "compiled"), "utf8");
  const sorted = (value: string) => value.trim().split("\n").sort();
  await build(first);
  const initial = await compiled();
  assert.ok(initial.includes("ips.cpp"));
  const metadata = JSON.parse(await readFile(join(out, "native-build.json"), "utf8"));
  assert.equal(metadata.cc, resolve("fixture-cc"));
  assert.equal(metadata.cxx, resolve("fixture-cxx"));
  await build(second);
  assert.equal(await compiled(), initial, "moving the snapshot must reuse every object");
  assert.equal(await readFile(join(out, "highway/source"), "utf8"), join(vendor, "highway"));
  await writeFile(join(out, "compiled"), "");
  await writeFile(join(second, "bench/rust/native/ips.cpp"), "changed shim");
  await build(second);
  assert.equal(await compiled(), "ips.cpp\n");
  assert.equal(await readFile(join(out, "tessera_native-ips.o"), "utf8"), "changed shim");
  await writeFile(join(out, "compiled"), "");
  await writeFile(join(second, "bench/rust/native/layout.h"), "changed layout");
  await build(second);
  assert.deepEqual(
    sorted(await compiled()),
    sorted(initial),
    "layout changes must rebuild every object",
  );
  await writeFile(join(out, "compiled"), "");
  await build(second, "fixture compiler 2");
  assert.deepEqual(
    sorted(await compiled()),
    sorted(initial),
    "compiler changes must rebuild every object",
  );
  await writeFile(join(out, "compiled"), "");
  const builder = join(second, "bench/rust/native/build.ts");
  await writeFile(builder, (await readFile(builder, "utf8")).replace('"-O3"', '"-O2"'));
  await build(second, "fixture compiler 2");
  assert.deepEqual(
    sorted(await compiled()),
    sorted(initial),
    "flag changes must rebuild every object",
  );
  await writeFile(join(out, "compiled"), "");
  await writeFile(join(out, "memory.o"), "");
  await build(second, "fixture compiler 2");
  assert.equal(await compiled(), "memory.cpp\n", "an empty object must be repaired");
});
