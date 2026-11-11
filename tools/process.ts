// SPDX-License-Identifier: BSD-2-Clause

import { AsyncLocalStorage } from "node:async_hooks";
import { spawn } from "node:child_process";
import { writeSync } from "node:fs";
import { open } from "node:fs/promises";
import { createInterface } from "node:readline";

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

/** An operator cancellation, distinct from an unsuccessful child exit. */
export class CommandCancelledError extends Error {
  constructor(signal: NodeJS.Signals) {
    super(`Operation cancelled by ${signal}`);
    this.name = "CommandCancelledError";
  }
}

interface CommandOptions {
  cwd?: string;
  env?: NodeJS.ProcessEnv;
  capture?: boolean;
  /** A line-oriented worker exchange, owned and cancelled with this command. */
  interact?: (exchange: (request: string) => Promise<string>) => Promise<void>;
}

/** Run a command, forwarding diagnostics and rejecting unsuccessful exits. */
export function command(
  program: string,
  args: string[],
  options: CommandOptions = {},
): Promise<string> {
  return new Promise((resolve, reject) => {
    // Nested helpers, including Cargo native builders, must remain inside
    // the outer command's cancellation boundary rather than detach again.
    const ownsGroup = process.platform !== "win32" && process.env.TESSERA_COMMAND_GROUP !== "1";
    const child = spawn(program, args, {
      detached: ownsGroup,
      cwd: options.cwd,
      env: { ...process.env, ...options.env, TESSERA_COMMAND_GROUP: "1" },
      stdio: [
        options.interact ? "pipe" : "ignore",
        options.capture || options.interact ? "pipe" : (commandLog.getStore() ?? "inherit"),
        commandLog.getStore() ?? "inherit",
      ],
    });
    let output = "";
    if (!options.interact)
      child.stdout?.setEncoding("utf8").on("data", (chunk: string) => {
        output += chunk;
      });
    const terminate = (signal: NodeJS.Signals) => {
      if (!ownsGroup) child.kill(signal);
      else if (child.pid) {
        try {
          process.kill(-child.pid, signal);
        } catch (error) {
          if ((error as NodeJS.ErrnoException).code !== "ESRCH") throw error;
        }
      }
    };
    let interactionError: unknown;
    const interaction = options.interact
      ? (async () => {
          const lines = createInterface({ input: child.stdout! })[Symbol.asyncIterator]();
          try {
            await options.interact!(async (request) => {
              child.stdin!.write(`${request}\n`);
              const line = await lines.next();
              if (line.done) throw new Error(`${program} closed its response stream`);
              return line.value;
            });
          } catch (error) {
            interactionError = error;
            terminate("SIGKILL");
          } finally {
            child.stdin!.end();
          }
        })()
      : Promise.resolve();
    child.stdin?.on("error", (error) => {
      interactionError ??= error;
    });
    let cancelled: CommandCancelledError | undefined;
    let escalation: ReturnType<typeof setTimeout> | undefined;
    const interrupt = (signal: NodeJS.Signals) => {
      if (cancelled) {
        terminate("SIGKILL");
        return;
      }
      cancelled = new CommandCancelledError(signal);
      terminate("SIGTERM");
      // Compilers and schedulers can own descendants; bound cancellation even
      // when one ignores SIGTERM or retains a captured stdout pipe.
      escalation = setTimeout(() => terminate("SIGKILL"), 1000);
      escalation.unref();
    };
    process.on("SIGINT", interrupt);
    process.on("SIGTERM", interrupt);
    child.once("error", reject);
    child.once("close", async (code, signal) => {
      await interaction;
      // Logged/inherited stdio can close with the direct child while a
      // descendant still runs. Finish cancellation before clearing its timer.
      if (cancelled) terminate("SIGKILL");
      if (escalation) clearTimeout(escalation);
      process.removeListener("SIGINT", interrupt);
      process.removeListener("SIGTERM", interrupt);
      if (cancelled) reject(cancelled);
      else if (interactionError) reject(interactionError);
      else if (code === 0) resolve(output.trim());
      else {
        if (output) diagnostic(output.trimEnd());
        reject(new Error(`${program} exited with ${signal ?? code}`));
      }
    });
  });
}
