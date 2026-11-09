// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { glob, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, matchesGlob, relative, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { titleErrors } from "./check_pr_title.ts";
import { checkDco } from "./check-dco.ts";
import { changedPaths, select } from "./ci_changes.ts";
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

test("title rules preserve accepted conventions and reject malformed summaries", () => {
  for (const title of [
    "refactor: clean up engine naming, documentation and tooling",
    "perf(engine): cap tile buckets at 4 per record above 8 bytes",
    "feat!: rename Sorter::sort_by_key to sort_by",
    "build(deps): bump serde from 1.0.0 to 1.0.1",
    "docs(site): explain the wide-record cursor cap",
  ])
    assert.deepEqual(titleErrors(title), []);
  for (const [title, error] of [
    ["Clean up engine naming", "expected"],
    ["refactor:clean up", "expected"],
    ["Refactor: clean up", "expected"],
    ["refactor(): clean up", "expected"],
    ["feature: add a sort", "type `feature`"],
    ["fix(lib): keep records", "scope `lib`"],
    ["fix: keep records.", "period"],
    [`docs: ${"x".repeat(60)}`, "over 64"],
    ["fix: change\n", "expected"],
    ["fix: first\nsecond", "expected"],
  ])
    assert.ok(
      titleErrors(title).some((message) => message.includes(error)),
      title,
    );
});

test("platform test cells enable library features without native competitors", async () => {
  const action = await readFile(
    new URL("../actions/rust-test/action.yml", import.meta.url),
    "utf8",
  );
  const validate = (action: string) => {
    const commands = [...action.matchAll(/^\s+run: (cargo (?:test|build)[^\n]+)$/gm)].map(
      (match) => match[1],
    );
    const workspace = commands.filter((command) => command.includes("--workspace"));
    assert.ok(workspace.length);
    for (const command of workspace) {
      assert.doesNotMatch(command, /--all-features/, command);
      const features = command.match(/--features\s+(\S+)/)?.[1].split(",") ?? [];
      assert.ok(
        features.every((feature) => feature.startsWith("tessera-sort/")),
        command,
      );
    }
    const combined = workspace.filter(
      (command) =>
        command.includes("tessera-sort/rayon") && command.includes("tessera-sort/portable-simd"),
    );
    assert.ok(
      combined.some(
        (command) => command.startsWith("cargo test") && command.includes("--lib --tests"),
      ),
    );
    assert.ok(combined.some((command) => command.includes("--doc")));
    assert.ok(combined.some((command) => command.startsWith("cargo build")));
    assert.doesNotMatch(commands.join("\n"), /compare-native/);
    assert.doesNotMatch(action, /TESSERA_BENCH_CHECK_ONLY=1/);
  };
  validate(action);
  const regression = action.replaceAll(
    "--features tessera-sort/portable-simd,tessera-sort/rayon",
    "--all-features",
  );
  assert.notEqual(regression, action);
  assert.throws(() => validate(regression), /all-features/);
});

test("language routing preserves independent and shared paths", () => {
  for (const paths of [
    ["rust/tessera-sort/src/lib.rs"],
    ["Cargo.lock"],
    ["rustfmt.toml"],
    ["bench/rust/main.rs"],
  ])
    assert.deepEqual(select(paths), { rust: true, cpp: false });
  for (const path of ["cpp/include/sort.hpp", "CMakeLists.txt"])
    assert.deepEqual(select([path]), { rust: false, cpp: true });
  for (const path of [
    "conformance/semantics.md",
    "conformance/vectors/a.json",
    "bench/report/index.ts",
    "tools/process.ts",
    "LICENSE",
    ".github/workflows/release.yml",
    ".github/workflows/rust-ci.yml",
  ])
    assert.deepEqual(select([path]), { rust: true, cpp: true });
  for (const path of [
    "docs/.vitepress/config.ts",
    "docs/public/logo.svg",
    "package.json",
    "pnpm-lock.yaml",
    "pnpm-workspace.yaml",
    ".github/workflows/docs.yml",
    "README.md",
    "RELEASING.md",
    "ROADMAP.md",
    "docs/explain/keys.md",
    "bench/README.md",
    "assets/logo.svg",
    ".github/scripts/ci_changes.ts",
    ".github/scripts/check-dco.sh",
    "biome.json",
  ])
    assert.deepEqual(select([path]), { rust: false, cpp: false });
  assert.deepEqual(select([]), { rust: false, cpp: false });
  assert.deepEqual(select(["rust/tessera-sort/README.md"]), {
    rust: true,
    cpp: false,
  });
  assert.deepEqual(select(["rust/a.rs", "cpp/b.hpp"]), { rust: true, cpp: true });
  assert.equal(changedPaths(undefined, "HEAD"), null);
  assert.equal(changedPaths("0".repeat(40), "HEAD"), null);
  assert.equal(changedPaths("missing-base-ref", "HEAD"), null);
});

test("Rust include paths remain covered by language routing", async () => {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
  for await (const path of glob("{rust,bench/rust}/**/*.rs", { cwd: root })) {
    const source = await readFile(join(root, path), "utf8");
    for (const match of source.matchAll(/include_(?:str|bytes)!\(\s*"([^"]+)"/g))
      assert.ok(select([relative(root, resolve(root, dirname(path), match[1]))]).rust, path);
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

test("title entry points retain JSON and actionable failure annotations", () => {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
  for (const args of [
    [join(root, ".github/scripts/check_pr_title.ts")],
    [join(root, "tools/index.ts"), "ci", "title"],
  ]) {
    const result = spawnSync(process.execPath, args, {
      encoding: "utf8",
      env: { ...process.env, PR_TITLE: "invalid title" },
    });
    assert.equal(result.status, 1);
    assert.ok(JSON.parse(result.stdout).errors.length > 0);
    assert.match(result.stderr, /::error::/);
    assert.match(result.stderr, /CONTRIBUTING.md/);
  }
});

test("benchmark triggers include runtime inputs without scheduling presentation changes", async () => {
  const source = await readFile(new URL("../workflows/bench-build.yml", import.meta.url), "utf8");
  const paths = source.split("    paths:\n")[1].split("  push:")[0];
  const patterns = [...paths.matchAll(/ {6}- "([^"\n]+)"/g)].map((match) => match[1]);
  const triggers = (path: string) =>
    patterns.reduce(
      (selected, pattern) =>
        matchesGlob(path, pattern.replace(/^!/, "")) ? !pattern.startsWith("!") : selected,
      false,
    );
  for (const path of [
    "rust/tessera-sort/src/lib.rs",
    "bench/rust/benches/smoke.rs",
    "bench/plans/continuous.toml",
    "bench/catalog.toml",
    "bench/workspace.ts",
    "tools/process.ts",
    "tools/continuous.ts",
    "bench/continuous/worker.ts",
    "Cargo.lock",
    ".cargo/config.toml",
  ])
    assert.ok(triggers(path), path);
  for (const path of [
    "bench/README.md",
    "rust/tessera-sort/README.md",
    "bench/rust/README.md",
    "bench/plot/rows.ts",
    "bench/continuous/chart.ts",
    "tools/status.ts",
    "package.json",
    "pnpm-lock.yaml",
    ".github/scripts/release.ts",
  ])
    assert.equal(triggers(path), false, path);
});
