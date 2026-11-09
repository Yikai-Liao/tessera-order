// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { chmod, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { parse, stringify } from "smol-toml";
import { command } from "../../tools/process.ts";
import { ALIGNMENT_RUSTFLAGS } from "../workspace.ts";
import { buildRevisions, serialBaselineManifest } from "./runner.ts";

test("pre-parallel serial baselines retain defaults and the shared locked dependency graph", async () => {
  const current = await readFile(
    new URL("../../rust/tessera-sort/Cargo.toml", import.meta.url),
    "utf8",
  );
  const historical = parse(current);
  delete (historical.features as Record<string, unknown>).rayon;
  delete (historical.dependencies as Record<string, unknown>).rayon;
  const baseline = stringify(historical);
  const completed = parse(serialBaselineManifest(baseline, current));
  assert.deepEqual(completed, parse(current));
  assert.deepEqual(
    (completed.features as Record<string, unknown>).default,
    (historical.features as Record<string, unknown>).default,
  );
  assert.equal(serialBaselineManifest(current, current), current);
  const conflicting = parse(baseline);
  (conflicting.dependencies as Record<string, unknown>).rayon = "1";
  assert.throws(() => serialBaselineManifest(stringify(conflicting), current), /explicit review/);
  const mandatory = parse(current);
  (mandatory.dependencies as Record<string, { optional: boolean }>).rayon.optional = false;
  assert.throws(() => serialBaselineManifest(baseline, stringify(mandatory)), /optional Rayon/);
});

test("layout controls append Cargo-resolved config flags to both revision builds", {
  skip: process.platform === "win32",
}, async (t) => {
  const root = await mkdtemp(join(tmpdir(), "tessera-control-flags-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  for (const path of ["rust/tessera-sort/src", "bench/rust/src", "bin"])
    await mkdir(join(root, path), { recursive: true });
  await writeFile(
    join(root, "rust/tessera-sort/Cargo.toml"),
    "[features]\nrayon = []\n[dependencies]\n",
  );
  await writeFile(join(root, "rust/tessera-sort/src/lib.rs"), "// fixture\n");
  await writeFile(join(root, "bench/rust/src/lib.rs"), "// harness\n");
  await writeFile(join(root, "bench/catalog.toml"), "schema = 1\n");
  await command("git", ["init", "-q", root]);
  await command("git", ["add", "."], { cwd: root });
  await command(
    "git",
    [
      "-c",
      "user.name=Test",
      "-c",
      "user.email=test@example.org",
      "-c",
      "commit.gpgsign=false",
      "-c",
      "core.hooksPath=/dev/null",
      "commit",
      "-qm",
      "fixture",
    ],
    { cwd: root },
  );
  const baseFlags = "--cfg\x1fproject_config\x1f-Ctarget-cpu=x86-64";
  const cargo = join(root, "bin/cargo");
  await writeFile(
    cargo,
    `#!${process.execPath}
    const fs = require("node:fs"), path = require("node:path");
    const target = process.argv[process.argv.indexOf("--target-dir") + 1];
    fs.mkdirSync(target, { recursive: true });
    const executable = path.join(target, "bench");
    fs.writeFileSync(executable, "executable");
    const flags = process.env.CARGO_ENCODED_RUSTFLAGS ?? ${JSON.stringify(baseFlags)};
    fs.appendFileSync(path.join(${JSON.stringify(root)}, "calls"), JSON.stringify(flags) + "\\n");
    fs.writeFileSync(path.join(target, "provenance.json"), JSON.stringify({
      rustc: "fixture", cargo_encoded_rustflags: flags, target: "x86_64-unknown-linux-gnu",
      profile: "release", opt_level: "3", debug: "true", features: []
    }));
    console.log(JSON.stringify({ reason: "compiler-artifact", package_id: "fixture", target: { name: "smoke" }, executable }));
    console.log(JSON.stringify({ reason: "build-script-executed", package_id: "fixture", out_dir: target }));
  `,
  );
  await chmod(cargo, 0o755);
  const saved = {
    PATH: process.env.PATH,
    RUSTFLAGS: process.env.RUSTFLAGS,
    CARGO_ENCODED_RUSTFLAGS: process.env.CARGO_ENCODED_RUSTFLAGS,
  };
  process.env.PATH = `${join(root, "bin")}:${saved.PATH}`;
  delete process.env.RUSTFLAGS;
  delete process.env.CARGO_ENCODED_RUSTFLAGS;
  try {
    for (const [control, flags] of [
      ["cgu1", ["-Ccodegen-units=1"]],
      ["aligned", ALIGNMENT_RUSTFLAGS],
    ] as const) {
      await writeFile(join(root, "calls"), "");
      const built = await buildRevisions(
        root,
        { directory: root, tree: root, target: join(root, "target") },
        "HEAD",
        [],
        undefined,
        control,
      );
      const expected = [...ALIGNMENT_RUSTFLAGS, baseFlags, ...flags].join("\x1f");
      assert.equal(built.builds.baseline.cargo_encoded_rustflags, expected);
      assert.equal(built.builds.current.cargo_encoded_rustflags, expected);
      assert.deepEqual(
        (await readFile(join(root, "calls"), "utf8"))
          .trim()
          .split("\n")
          .map((line) => JSON.parse(line)),
        [baseFlags, [...ALIGNMENT_RUSTFLAGS, baseFlags].join("\x1f"), expected, expected],
      );
    }
  } finally {
    for (const [key, value] of Object.entries(saved)) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  }
});
