// SPDX-License-Identifier: BSD-2-Clause

import { createHash } from "node:crypto";
import { copyFile, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { basename, dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { command } from "../../../tools/process.ts";
import { nativeSources, verifySources } from "./sources.ts";

const root = dirname(fileURLToPath(import.meta.url));
// Compiler inputs stay relative to this directory across isolated source snapshots.
const vendor = "../../vendor";
const base = ["-O3", "-DNDEBUG", "-fPIC", "-march=x86-64", "-mtune=generic"];

interface TranslationUnit {
  source: string;
  compiler: string;
  flags: string[];
  object: string;
}

/** Compile pinned sequential native adapters without network access or host tuning. */
async function build(out: string) {
  const sources = await nativeSources();
  await verifySources(resolve(root, vendor), sources);
  for (const variable of ["CFLAGS", "CXXFLAGS", "LDFLAGS"])
    if (process.env[variable])
      throw new Error(`${variable} must be unset; native comparison uses fixed flags`);
  // Preserve caller-relative compiler overrides when compilation changes cwd.
  const [cc, cxx] = [process.env.CC ?? "clang", process.env.CXX ?? "clang++"].map((compiler) =>
    compiler.includes("/") ? resolve(compiler) : compiler,
  );
  const compilers = new Map<string, string>();
  for (const compiler of new Set([cc, cxx]))
    compilers.set(compiler, await command(compiler, ["--version"], { capture: true }));
  const jobs = Math.min(2, Number(process.env.CARGO_BUILD_JOBS ?? 2));
  if (!Number.isInteger(jobs) || jobs < 1) throw new Error("CARGO_BUILD_JOBS must be positive");
  const highway = join(out, "highway");
  await command("cmake", [
    "-S",
    // The workspace's vendor link points to the retained, verified sources.
    // CMake can reuse its dependency graph when the snapshot directory changes.
    await realpath(resolve(root, vendor, "highway")),
    "-B",
    highway,
    "-DCMAKE_BUILD_TYPE=Release",
    "-DBUILD_SHARED_LIBS=OFF",
    "-DHWY_ENABLE_TESTS=OFF",
    "-DHWY_ENABLE_EXAMPLES=OFF",
    "-DHWY_ENABLE_CONTRIB=OFF",
    "-DHWY_ENABLE_IO=OFF",
    `-DCMAKE_CXX_COMPILER=${cxx}`,
    `-DCMAKE_CXX_FLAGS=${base.join(" ")}`,
  ]);
  await command("cmake", ["--build", highway, "--target", "hwy", "-j", String(jobs)]);
  await copyFile(join(highway, "libhwy.a"), join(out, "libhwy.a"));
  const cpp = ["-std=c++17", "-I.", `-I${vendor}/highway`, `-I${vendor}/x86simdsort/lib`];
  const tasks: TranslationUnit[] = [];
  const libs: Record<string, string[]> = { tessera_native: [], hwy_contrib: [], x86simdsort: [] };
  const add = (library: string, source: string, compiler: string, flags: string[]) => {
    const object = join(out, `${library}-${basename(source).replace(/\.[^.]+$/, "")}.o`);
    tasks.push({ source, compiler, flags, object });
    libs[library].push(object);
  };
  for (const name of ["registry", "simd", "ips"])
    add("tessera_native", `${name}.cpp`, cxx, [
      ...cpp,
      ...(name === "ips"
        ? ["-U_REENTRANT", `-I${vendor}/ips2ra/include`, `-I${vendor}/ips4o/include`]
        : []),
    ]);
  add("tessera_native", "duckdb.cpp", cxx, [
    ...cpp,
    ...["vergesort", "ska_sort", "pdqsort"].map((name) => `-I${vendor}/duckdb/third_party/${name}`),
  ]);
  for (const name of ["fluxsort", "crumsort"])
    add("tessera_native", `${name}.c`, cc, ["-std=c11", `-I${vendor}/${name}/src`, "-I."]);
  add("tessera_native", "clausecker.c", cc, [`-I${vendor}/clausecker`]);
  add("tessera_native", join(vendor, "clausecker/radixsort_permuted.c"), cc, []);
  for (const name of ["vqsort", "vqsort_u32a", "vqsort_u64a"])
    add("hwy_contrib", join(vendor, `highway/hwy/contrib/sort/${name}.cc`), cxx, cpp);
  for (const [name, arch] of [
    ["x86simdsort", "x86-64"],
    ["x86simdsort-avx2", "haswell"],
    ["x86simdsort-skx", "skylake-avx512"],
    ["x86simdsort-icl", "icelake-client"],
    ["x86simdsort-spr", "sapphirerapids"],
  ])
    add("x86simdsort", join(vendor, `x86simdsort/lib/${name}.cpp`), cxx, [
      ...cpp,
      `-I${vendor}/x86simdsort/src`,
      `-I${vendor}/x86simdsort/utils`,
      `-march=${arch}`,
    ]);
  tasks.push({
    source: "memory.cpp",
    compiler: cxx,
    flags: cpp,
    object: join(out, "memory.o"),
  });

  const compile = async ({ source, compiler, flags, object }: TranslationUnit) => {
    const args = [...base, ...flags, "-c", source, "-o", object];
    const digest = createHash("sha256")
      .update(JSON.stringify([[compiler, ...args], sources, compilers.get(compiler)]))
      .update(await readFile(resolve(root, source)))
      .update(await readFile(join(root, "layout.h")))
      .digest("hex");
    const stamp = object.replace(/\.o$/, ".sha256");
    try {
      if ((await readFile(stamp, "utf8")) === digest && (await readFile(object)).length) return;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
    }
    await command(compiler, args, { cwd: root });
    await writeFile(stamp, digest);
  };
  // Each lane owns its queue; settle both before exiting on a compiler failure.
  const lanes = Array.from({ length: jobs }, (_, lane) =>
    tasks.filter((_, index) => index % jobs === lane),
  );
  const results = await Promise.allSettled(
    lanes.map(async (tasks) => {
      for (const task of tasks) await compile(task);
    }),
  );
  for (const result of results) if (result.status === "rejected") throw result.reason;
  for (const name of ["fluxsort", "crumsort"])
    await command("objcopy", [
      "--wildcard",
      `--keep-global-symbol=bench_${name}_*`,
      join(out, `tessera_native-${name}.o`),
    ]);
  for (const [name, objects] of Object.entries(libs)) {
    const archive = join(out, `lib${name}.a`);
    await rm(archive, { force: true });
    await command(process.env.AR ?? "ar", ["rcs", archive, ...objects]);
  }
  await writeFile(
    join(out, "native-build.json"),
    JSON.stringify(
      {
        cc,
        cxx,
        flags: base,
        translation_units: tasks.map(({ source, compiler, flags }) => ({
          source: relative(resolve(root, "../../.."), resolve(root, source)),
          compiler,
          flags: [...base, ...flags],
        })),
        ips_parallel: false,
        libraries: Object.keys(libs),
      },
      null,
      2,
    ),
  );
}

await build(resolve(process.argv[2]));
