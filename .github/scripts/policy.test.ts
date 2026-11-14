// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { test } from "node:test";
import { checkDco } from "./check-dco.ts";
import { changedPaths, changesTrustedFiles, findMarker, marker, select } from "./ci_changes.ts";
import {
  BOT_EMAIL,
  isDependabotUpdate,
  isVerifiedDependabot,
  verifyCommit,
} from "./verify_dependabot.ts";

const sha = "a".repeat(40);
const bot = { id: 49699333, login: "dependabot[bot]", type: "Bot" };
const verified = {
  sha,
  author: bot,
  committer: { id: 19864447, login: "web-flow" },
  commit: {
    author: { email: BOT_EMAIL },
    committer: { email: "noreply@github.com" },
    verification: { verified: true, reason: "valid" },
  },
};
const event = {
  action: "synchronize",
  after: sha,
  sender: bot,
  repository: { id: 42, full_name: "owner/repo" },
  pull_request: { user: bot, head: { sha, repo: { id: 42 } }, base: { repo: { id: 42 } } },
};

/** Replace one nested field without mutating the shared authenticated fixtures. */
function replace(value: object, path: string[], replacement: unknown) {
  const copy = structuredClone(value) as Record<string, unknown>;
  let field = copy;
  for (const key of path.slice(0, -1)) field = field[key] as Record<string, unknown>;
  field[path.at(-1)!] = replacement;
  return copy;
}

test("routing keeps library, comparison and revision checks with their owners", () => {
  const none = { rust: false, cpp: false, comparison: false, benchmark: false, docs: false };
  const owners: [string[], Partial<typeof none>][] = [
    [
      ["rust/tessera-order/src/lib.rs", "rust/tessera-order/Cargo.toml"],
      { rust: true, benchmark: true, docs: true },
    ],
    [["rust/tessera-trace/src/lib.rs", "rust/tessera-trace/Cargo.lock"], { docs: true }],
    [
      ["rust/tessera-order/tests/memory.rs", ".github/actions/rust-test/action.yml"],
      { rust: true },
    ],
    [
      ["Cargo.lock", "Cargo.toml", ".cargo/config.toml", "rust-toolchain.toml"],
      { rust: true, comparison: true, benchmark: true },
    ],
    [["cpp/include/sort.hpp", ".github/workflows/cpp-ci.yml"], { cpp: true }],
    [
      [
        "bench/rust/native/ips.cpp",
        "bench/rust/src/allocation.rs",
        "bench/rust/src/bin/compare.rs",
        "bench/rust/src/bin/memory.rs",
        "bench/rust/tests/comparison.rs",
      ],
      { comparison: true },
    ],
    [
      [
        "bench/rust/src/case.rs",
        "bench/rust/build.rs",
        "bench/workspace.ts",
        "tools/process.ts",
        "bench/rust/src/comparison/record.rs",
        "bench/rust/src/comparison/variant.rs",
        "bench/rust/src/comparison/run.rs",
        "bench/release.ts",
        "bench/plans/continuous.toml",
        "bench/plans/local.toml",
        "bench/plans/smoke.toml",
      ],
      { comparison: true, benchmark: true },
    ],
    [
      [
        "bench/continuous/paired.ts",
        "bench/local/runner.ts",
        "tools/continuous.ts",
        "bench/tracking/run.ts",
        "bench/tracking/build.ts",
        "bench/tracking/jobs.ts",
        "bench/tracking/publish.ts",
        "bench/tracking/run.sh",
        "bench/tracking/Dockerfile",
      ],
      { benchmark: true },
    ],
    [["bench/tracking/result.ts", "bench/tracking/history.ts"], { benchmark: true, docs: true }],
    [["docs/explain/keys.md", "conformance/semantics.md"], { docs: true }],
    [["package.json", "bun.lock"], { comparison: true, benchmark: true, docs: true }],
    [
      [
        "tools/status.ts",
        "tools/code.ts",
        "bench/code-change.ts",
        "bench/code-source.ts",
        "tools/repository.ts",
        "biome.json",
        "tsconfig.json",
        "bench/continuous/markdown.ts",
        "bench/continuous/index.test.ts",
        "bench/rust/native/build.test.ts",
        "bench/plans/diagnostics.toml",
        "fuzz/fuzz_targets/sort.rs",
        ".github/dependabot.yml",
        ".github/pull_request_template.md",
        ".github/workflows/dco.yml",
        ".github/workflows/pr-title.yml",
        ".github/workflows/release.yml",
        ".github/workflows/rust-release.yml",
        ".github/workflows/cpp-release.yml",
        ".github/workflows/bench-diagnostics.yml",
        ".github/workflows/main-benchmarks.yml",
      ],
      {},
    ],
  ];
  for (const [paths, expected] of owners)
    for (const path of paths) assert.deepEqual(select([path]), { ...none, ...expected }, path);
  // A path no row names, and an unavailable base, select every lane.
  for (const path of ["unknown-input", "conformance/vectors/keys.json", ".gitignore"])
    assert.ok(Object.values(select([path])).every(Boolean), path);
  assert.deepEqual(select(["rust/tessera-order/src/lib.rs", "bench/rust/native/ips.cpp"]), {
    ...none,
    rust: true,
    comparison: true,
    benchmark: true,
    docs: true,
  });
  assert.equal(changedPaths(undefined, "HEAD"), null);
  assert.equal(changedPaths("missing-base-ref", "HEAD"), null);
});

test("reuse markers follow lane inputs and trusted files block reuse", async () => {
  const root = await mkdtemp(join(tmpdir(), "tessera-reuse-"));
  const previous = process.cwd();
  const git = (...args: string[]) =>
    execFileSync("git", ["-c", "user.name=t", "-c", "user.email=t@t", ...args], {
      cwd: root,
      encoding: "utf8",
    }).trim();
  const commit = async (path: string, text: string) => {
    await mkdir(join(root, path, ".."), { recursive: true });
    await writeFile(join(root, path), text);
    git("add", "-A");
    git("commit", "-qm", path);
    return git("rev-parse", "HEAD");
  };
  try {
    git("init", "-q");
    await commit("rust/tessera-order/src/lib.rs", "a");
    const base = await commit("docs/guide.md", "a");
    const docs = await commit("docs/guide.md", "b");
    const workflow = await commit(".github/workflows/ci.yml", "b");
    process.chdir(root);
    const at = (tested: string) => ({ tested, head: tested, baseline: base });
    assert.equal(marker("rust", at(base)), marker("rust", at(docs)));
    assert.notEqual(marker("docs", at(base)), marker("docs", at(docs)));
    // Measurement also depends on the baseline it compares against.
    assert.notEqual(
      marker("benchmark", at(base)),
      marker("benchmark", { ...at(base), baseline: workflow }),
    );
    // A marker counts only from a run on the PR's own commits; any failure finds none.
    const github = { repository: "owner/repo", token: "" };
    const respond = (artifacts: unknown) => async () => Response.json({ artifacts });
    const run = (head_sha: string) => ({ expired: false, workflow_run: { id: 7, head_sha } });
    const original = globalThis.fetch;
    try {
      const found = async (reply: () => Promise<Response>) => {
        globalThis.fetch = reply as typeof fetch;
        return findMarker("m", { head: workflow, baseline: base }, github);
      };
      assert.deepEqual(await found(respond([run(docs)])), { id: 7, head_sha: docs });
      assert.equal(await found(respond([])), undefined);
      assert.equal(await found(respond([run(base)])), undefined);
      assert.equal(await found(respond([{ ...run(docs), expired: true }])), undefined);
      assert.equal(await found(async () => new Response("", { status: 403 })), undefined);
      assert.equal(await found(() => Promise.reject(new Error("offline"))), undefined);
    } finally {
      globalThis.fetch = original;
    }
    assert.equal(changesTrustedFiles(base, docs), false);
    assert.equal(changesTrustedFiles(base, workflow), true);
  } finally {
    process.chdir(previous);
    await rm(root, { recursive: true, force: true });
  }
});

test("Dependabot provenance requires every event and identity field", () => {
  for (const action of ["opened", "synchronize"])
    assert.ok(isDependabotUpdate({ ...event, action }, sha, "owner/repo"));
  for (const [path, replacement] of [
    [["action"], "reopened"],
    [["after"], "b".repeat(40)],
    [["after"], null],
    [["sender"], null],
    [["sender", "id"], 123],
    [["sender", "login"], "someone"],
    [["sender", "type"], "User"],
    [["repository"], null],
    [["repository", "id"], 43],
    [["repository", "id"], true],
    [["repository", "full_name"], "attacker/repo"],
    [["pull_request", "user"], null],
    [["pull_request", "user", "id"], 123],
    [["pull_request", "user", "login"], "someone"],
    [["pull_request", "user", "type"], "User"],
    [["pull_request", "head", "repo"], null],
    [["pull_request", "head", "repo", "id"], 43],
    [["pull_request", "base", "repo", "id"], 43],
    [["pull_request", "head", "sha"], "b".repeat(40)],
  ] as [string[], unknown][])
    assert.equal(
      isDependabotUpdate(replace(event, path, replacement), sha, "owner/repo"),
      false,
      path.join("."),
    );
  assert.equal(isDependabotUpdate(event, "b".repeat(40), "owner/repo"), false);
  assert.ok(isVerifiedDependabot(verified, sha));
  for (const [path, replacement] of [
    [["sha"], "b".repeat(40)],
    [["author"], null],
    [["author", "id"], 123],
    [["author", "login"], "renovate[bot]"],
    [["author", "type"], "User"],
    [["committer"], null],
    [["committer", "id"], 123],
    [["committer", "login"], "someone"],
    [["commit", "author", "email"], "someone@example.com"],
    [["commit", "committer", "email"], "someone@example.com"],
    [["commit", "verification"], null],
    [["commit", "verification", "verified"], false],
    [["commit", "verification", "verified"], "true"],
    [["commit", "verification", "reason"], "unsigned"],
    [["commit", "verification", "reason"], "bad_email"],
  ] as [string[], unknown][])
    assert.equal(
      isVerifiedDependabot(replace(verified, path, replacement), sha),
      false,
      path.join("."),
    );
  for (const value of [null, [], {}, "bot", { commit: [] }]) {
    assert.equal(isVerifiedDependabot(value, sha), false);
    assert.equal(isDependabotUpdate(value, sha, "owner/repo"), false);
  }
});

test("verification requests fail closed and never query untrusted destinations", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-dco-event-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const path = join(directory, "event.json");
  await writeFile(path, JSON.stringify(event));
  const env = {
    GITHUB_REPOSITORY: "owner/repo",
    GH_TOKEN: "test-token",
    GITHUB_EVENT_PATH: path,
    GITHUB_EVENT_NAME: "pull_request",
    GITHUB_ACTOR_ID: "49699333",
  };
  let calls = 0;
  const fetcher: typeof fetch = async (url, init) => {
    calls++;
    assert.equal(url, `https://api.github.com/repos/owner/repo/commits/${sha}`);
    assert.equal((init!.headers as Record<string, string>).Authorization, "Bearer test-token");
    assert.ok(init!.signal);
    return Response.json(verified);
  };
  assert.equal(await verifyCommit(sha, env, fetcher), true);
  assert.equal(
    await verifyCommit(sha, { ...env, GITHUB_TRIGGERING_ACTOR: "maintainer" }, fetcher),
    true,
  );
  for (const override of [
    { GH_TOKEN: "" },
    { GITHUB_ACTOR_ID: "" },
    { GITHUB_ACTOR_ID: "123" },
    { GITHUB_EVENT_NAME: "workflow_dispatch" },
    ...["", "owner/repo/../../other", "https://example.com", "owner/repo?token"].map(
      (GITHUB_REPOSITORY) => ({ GITHUB_REPOSITORY }),
    ),
  ])
    assert.equal(await verifyCommit(sha, { ...env, ...override }, fetcher), false);
  for (const invalid of ["main", "../main", `${sha}?`, ""])
    assert.equal(await verifyCommit(invalid, env, fetcher), false);
  assert.equal(calls, 2);
  for (const invalid of ["not json", "null", "[]", "{}", '{"sender":"bot"}']) {
    await writeFile(path, invalid);
    assert.equal(await verifyCommit(sha, env, fetcher), false);
  }
  assert.equal(calls, 2);
  await writeFile(path, JSON.stringify(event));
  for (const text of ["not json", "null", "[]", "{}", '{"commit":[]}'])
    assert.equal(await verifyCommit(sha, env, async () => new Response(text)), false);
  for (const status of [403, 500])
    assert.equal(await verifyCommit(sha, env, async () => new Response("", { status })), false);
  assert.equal(
    await verifyCommit(sha, env, async () => {
      throw new Error("timeout");
    }),
    false,
  );
  await rm(path);
  assert.equal(await verifyCommit(sha, env, fetcher), false);
});

test("DCO enforces each commit independently of a later sign-off or bot exception", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "tessera-dco-repo-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const before = process.cwd();
  const git = (...args: string[]) =>
    execFileSync(
      "git",
      [
        "-c",
        "user.name=Contributor",
        "-c",
        "user.email=contributor@example.com",
        "-c",
        "commit.gpgsign=false",
        "-c",
        "core.hooksPath=/dev/null",
        ...args,
      ],
      { cwd: root, encoding: "utf8" },
    ).trim();
  git("init", "-q");
  git("commit", "--allow-empty", "-qm", "base");
  const base = git("rev-parse", "HEAD");
  try {
    process.chdir(root);
    git(
      "commit",
      "--allow-empty",
      "-qm",
      "signed\n\nSigned-off-by: Contributor <contributor@example.com>",
    );
    assert.deepEqual(await checkDco(base, "HEAD"), []);
    git(
      "commit",
      "--allow-empty",
      "-qm",
      "dependency",
      "--author",
      `dependabot[bot] <${BOT_EMAIL}>`,
    );
    assert.equal((await checkDco(base, "HEAD", async () => false)).length, 1);
    assert.deepEqual(await checkDco(base, "HEAD", async () => true), []);
    git("commit", "--allow-empty", "-qm", "unsigned human");
    assert.equal((await checkDco(base, "HEAD", async () => true)).length, 1);
    for (const args of [
      [resolve(before, ".github/scripts/check-dco.ts"), base, "HEAD"],
      [resolve(before, "tools/index.ts"), "ci", "dco", base, "HEAD"],
    ]) {
      const result = spawnSync(process.execPath, args, {
        cwd: root,
        encoding: "utf8",
        env: { ...process.env, GITHUB_EVENT_NAME: "" },
      });
      assert.equal(result.status, 1);
      assert.equal(JSON.parse(result.stdout).missing.length, 2);
      assert.match(result.stderr, /::error::.*no Signed-off-by/);
      assert.match(result.stderr, /git commit -s.*CONTRIBUTING.md/);
    }
    git(
      "commit",
      "--allow-empty",
      "-qm",
      "later\n\nSigned-off-by: Contributor <contributor@example.com>",
    );
    assert.equal((await checkDco(base, "HEAD", async () => true)).length, 1);
    git(
      "commit",
      "--allow-empty",
      "-qm",
      "other bot",
      "--author",
      "renovate[bot] <renovate@example.com>",
    );
    assert.equal((await checkDco(base, "HEAD", async () => true)).length, 2);
  } finally {
    process.chdir(before);
  }
});
