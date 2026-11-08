// SPDX-License-Identifier: BSD-2-Clause

import { extname } from "node:path";
import { parse as parseToml } from "smol-toml";

/** Decode a configuration document without interpreting its domain fields.
 *
 * The filename selects JSON or TOML. Callers validate the decoded value with
 * their configuration schema; syntax failures retain the filename and cause.
 */
export function decodeConfig(text: string, filename: string): unknown {
  const extension = extname(filename).toLowerCase();
  if (extension !== ".json" && extension !== ".toml")
    throw new Error(`Unsupported configuration format: ${filename}; expected .json or .toml`);
  try {
    return extension === ".json" ? JSON.parse(text) : parseToml(text);
  } catch (cause) {
    throw new Error(`Cannot parse ${filename}: ${cause instanceof Error ? cause.message : cause}`, {
      cause,
    });
  }
}
