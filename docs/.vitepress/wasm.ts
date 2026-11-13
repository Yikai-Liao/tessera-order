// SPDX-License-Identifier: BSD-2-Clause
// Builds `rust/tessera-trace` for WebAssembly and copies it next to the
// figure models. It runs before the dev, test and build scripts, so the figures
// always replay the engine in this checkout. Requires cargo and the
// `wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`).

import { execFileSync } from "node:child_process";
import { copyFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../..", import.meta.url));
const target = "wasm32-unknown-unknown";
try {
  execFileSync(
    "cargo",
    [
      "build",
      "--quiet",
      "--release",
      "--manifest-path",
      "rust/tessera-trace/Cargo.toml",
      "--target",
      target,
      "--target-dir",
      "target/trace",
    ],
    { cwd: root, stdio: "inherit" },
  );
} catch (error) {
  console.error(
    `Building the figure engine failed. It needs cargo and \`rustup target add ${target}\`.`,
  );
  throw error;
}
copyFileSync(
  `${root}/target/trace/${target}/release/tessera_trace.wasm`,
  `${root}/docs/.vitepress/theme/sim/tessera_trace.wasm`,
);
