// SPDX-License-Identifier: BSD-2-Clause

import { AsyncLocalStorage } from "node:async_hooks";
import { spawn } from "node:child_process";
import { writeSync } from "node:fs";
import { open } from "node:fs/promises";

const commandLog = new AsyncLocalStorage<number>();

/** Write progress alongside command diagnostics, never into the machine result. */
export function diagnostic(message: string) {
  const fd = commandLog.getStore();
  if (fd === undefined) process.stderr.write(`${message}\n`);
  else writeSync(fd, `${message}\n`);
}

/** Keep subprocess output in one log for the lifetime of a complete operation. */
export async function withCommandLog<T>(path: string, operation: () => Promise<T>): Promise<T> {
  const log = await open(path, "wx");
  try {
    return await commandLog.run(log.fd, operation);
  } finally {
    await log.close();
  }
}

interface CommandOptions {
  cwd?: string;
  env?: NodeJS.ProcessEnv;
  capture?: boolean;
}

/** Run a command, forwarding diagnostics and rejecting unsuccessful exits. */
export function command(
  program: string,
  args: string[],
  options: CommandOptions = {},
): Promise<string> {
  return new Promise((resolve, reject) => {
    const child = spawn(program, args, {
      cwd: options.cwd,
      env: { ...process.env, ...options.env },
      stdio: [
        "ignore",
        options.capture ? "pipe" : (commandLog.getStore() ?? "inherit"),
        commandLog.getStore() ?? "inherit",
      ],
    });
    let output = "";
    child.stdout?.setEncoding("utf8").on("data", (chunk: string) => {
      output += chunk;
    });
    const interrupt = () => child.kill("SIGTERM");
    process.once("SIGINT", interrupt);
    process.once("SIGTERM", interrupt);
    child.once("error", reject);
    child.once("close", (code, signal) => {
      process.removeListener("SIGINT", interrupt);
      process.removeListener("SIGTERM", interrupt);
      if (code === 0) resolve(output.trim());
      else reject(new Error(`${program} exited with ${signal ?? code}`));
    });
  });
}
