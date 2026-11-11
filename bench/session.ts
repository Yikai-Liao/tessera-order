// SPDX-License-Identifier: BSD-2-Clause

import { createHash } from "node:crypto";
import { copyFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { basename, dirname, join } from "node:path";
import lockfile from "proper-lockfile";
import { z } from "zod";
import { cpuListSchema } from "./affinity.ts";
import { findPairedBudget } from "./statistics.ts";

const digest = z.string().regex(/^[a-f0-9]{64}$/);
const schema = z.strictObject({
  schema: z.literal(2),
  platform: z.string(),
  planSha256: digest,
  cpu: cpuListSchema.optional(),
  group: z.string().optional(),
  sampleMs: z.number().int().positive(),
  blocks: z
    .number()
    .refine((count) => findPairedBudget(count) !== undefined, "unsupported paired-block budget")
    .optional(),
  memoryMib: z.number().int().positive(),
  build: z.strictObject({
    compareSha256: digest,
    memorySha256: digest,
  }),
});

/** Immutable measurement settings and executable identities for one session. */
export type Session = z.infer<typeof schema>;
export type Build = Session["build"];

/** Allow one writer per session, reclaiming abandoned locks after the lease expires. */
export async function withSessionLock<T>(out: string, operation: () => Promise<T>): Promise<T> {
  const release = await lockfile.lock(out, { lockfilePath: join(out, ".session-lock") });
  try {
    return await operation();
  } finally {
    await release();
  }
}

/** Hash persisted bytes, including executable contents rather than file names. */
export async function fileHash(path: string) {
  return createHash("sha256")
    .update(await readFile(path))
    .digest("hex");
}

/** Resolve the retained executable pair without depending on a temporary worktree. */
export function executables(out: string) {
  const suffix = process.platform === "win32" ? ".exe" : "";
  const directory = join(out, "bin");
  return {
    compare: join(directory, `compare${suffix}`),
    memory: join(directory, `memory${suffix}`),
  };
}

/** Retain both executable roles before removing the temporary build tree. */
export async function retainBuild(out: string, binary: string): Promise<Build> {
  const paths = executables(out);
  await mkdir(dirname(paths.compare), { recursive: true });
  await copyFile(binary, paths.compare);
  await copyFile(join(dirname(binary), basename(paths.memory)), paths.memory);
  return {
    compareSha256: await fileHash(paths.compare),
    memorySha256: await fileHash(paths.memory),
  };
}

/** Load a prepared session; reject changed plans, binaries or host platforms before execution. */
export async function loadSession(out: string): Promise<Session> {
  let document: string;
  try {
    document = await readFile(join(out, "session.json"), "utf8");
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT")
      throw new Error(
        `No prepared session in ${out}; preparation must finish before resume is available`,
      );
    throw error;
  }
  const session = schema.parse(JSON.parse(document));
  if (session.platform !== process.platform)
    throw new Error("Session platform differs from this host");
  if ((await fileHash(join(out, "plan.json"))) !== session.planSha256)
    throw new Error("Session plan changed; start a new experiment instead");
  const paths = executables(out);
  if (
    (await fileHash(paths.compare)) !== session.build.compareSha256 ||
    (await fileHash(paths.memory)) !== session.build.memorySha256
  )
    throw new Error("Session executables changed; start a new experiment instead");
  return session;
}

/** Skip validation only when its receipt matches this immutable build and selection. */
export async function isValidated(out: string, build: Build, session: Session) {
  const path = join(out, "bin", "validated.json");
  try {
    return (await readFile(path, "utf8")) === validationReceipt(build, session);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return false;
    throw error;
  }
}

/** Bind a completed validation to the binary, plan and optional group restriction. */
function validationReceipt(build: Build, session: Session) {
  return `${JSON.stringify({ build, planSha256: session.planSha256, group: session.group, blocks: session.blocks })}\n`;
}

/** Persist a receipt only after every selected input has passed validation. */
export async function recordValidation(out: string, build: Build, session: Session) {
  await writeFile(join(out, "bin", "validated.json"), validationReceipt(build, session));
}
