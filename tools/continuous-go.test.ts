// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { GO_MODULE, goReleaseVersion } from "../.github/scripts/release.ts";
import { trackingSelection } from "../bench/tracking/implementations.ts";
import { goShardCount } from "./continuous.ts";

test("the Go matrix scales from one cell to four", () => {
  const id = "go/sort/u64/random/1000000/s1/serial";
  assert.equal(goShardCount([id]), 1);
  assert.equal(goShardCount(Array(13).fill(id)), 2);
  assert.equal(goShardCount(Array(100).fill(id)), 4);
});

test("mixed main pushes choose only the two Tessera implementations", () => {
  assert.deepEqual(trackingSelection("tessera,go-tessera", ["serial", "t4"]), [
    { implementation: "tessera", execution: "serial" },
    { implementation: "tessera", execution: "t4" },
    { implementation: "go-tessera", execution: "serial" },
    { implementation: "go-tessera", execution: "t4" },
  ]);
});

test("Go release validation requires the exact module and a canonical v0 or v1 tag", () => {
  const manifest = `module ${GO_MODULE}\ngo 1.27\n`;
  assert.equal(goReleaseVersion("go/tessera/v0.1.0", manifest), "v0.1.0");
  assert.equal(goReleaseVersion("go/tessera/v1.4.2-rc.1", manifest), "v1.4.2-rc.1");
  for (const tag of [
    "rust/v0.1.0",
    "go/tessera/v2.0.0",
    "go/tessera/v01.0.0",
    "go/tessera/v1.01.0",
    "go/tessera/v1.2",
    "go/tessera/v1.0.0-rc.01",
    "go/tessera/v1.0.0+meta",
  ])
    assert.throws(() => goReleaseVersion(tag, manifest), /canonical/);
  assert.throws(
    () => goReleaseVersion("go/tessera/v1.0.0", "module example.com/other"),
    /path mismatch/,
  );
});
