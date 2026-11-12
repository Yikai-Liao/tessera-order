// SPDX-License-Identifier: BSD-2-Clause

import { createRequire } from "node:module";
import { run } from "vue-tsc";

// vue-tsc needs the JS compiler API; keep native TypeScript 7 for the CLI tooling.
const require = createRequire(import.meta.url);
run(require.resolve("typescript6/lib/tsc"));
