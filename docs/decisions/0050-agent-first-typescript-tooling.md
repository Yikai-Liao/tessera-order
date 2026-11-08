<!-- SPDX-License-Identifier: BSD-2-Clause -->

# 0050: Agent-first repository tooling in TypeScript

Date: 2026-11-08. Status: accepted.

## Context

Benchmark development, algorithm comparisons and continuous tracking share experiment plans but still expose separate build and CI procedures. The previous benchmark CLI could read a PR, not the default branch's exact current CI state. Verbose process output and full GitHub payloads make routine agent reads expensive. Small Python scripts also introduce a second tooling runtime for routing, release validation, native compilation and cloud scheduling.

Moving every workflow into a CLI would hide permission and event boundaries that belong to GitHub Actions. Keeping each workflow's shell procedure as an independent local implementation would duplicate build flags, scheduling rules and failure handling.

## Decision

Use Node.js 24 and the pinned pnpm toolchain for repository tooling. Group commands by task under `pnpm tools`: source and algorithm comparisons, plans, reports, functional checks, continuous execution, feedback, repository policy and release validation. Remove the old benchmark command and aliases rather than carrying compatibility for the superseded interface.

Give complete operations ownership of source snapshots, build directories, child processes and cleanup. CI and local commands call the same native checks, static artifact builder, continuous scheduler and Bencher submission functions. Keep image publication, credentials, queue ordering, event attribution and GitHub report publication at the workflow boundary.

Make feedback read-only and compact JSON by default. Resolve the remote target once, associate PR test-merge workflows separately from head-attached benchmark checks, and identify the trusted tracker through its published check identity. Workflow success does not stand in for a benchmark verdict. Full summaries and comments are opt-in. Build and measurement commands retain diagnostics in an operation-scoped log and emit one small result.

Replace repository Python scripts and inline release helpers with TypeScript. Keep routing, title and DCO bootstrap entry points dependency-free: DCO must execute the trusted base checkout without installing or running PR code. Reuse the same policy functions from the CLI. Use the tar library for verified archive extraction rather than implementing an archive parser.

## Consequences

Agents can inspect main or a PR without loading historical reports or compiler output, and developers can reproduce the benchmark-related CI operations locally. Failures retain their logs, both remote configurations are attempted, and missing feedback is not reported as a pass.

The CLI and CI require Node.js 24; native competitors still require their documented C/C++ toolchain. Python is no longer a repository-tooling dependency. Unimplemented Python CI and publication scaffolding are removed; they will be designed alongside an actual binding. The cloud image now carries Node.js and the trusted TypeScript worker; executables still run without API credentials. The privileged tracker adopts these changes only after they reach the default branch.

## Migration

The old `pnpm bench` entry point and its `run`, `release` and `pr` commands are removed. Use `pnpm tools compare commit`, `pnpm tools compare algorithms`, `pnpm tools status --pr NUMBER` and `pnpm tools report`. JSON is the default status format, replacing the old `--json` flag. Ruff and the Python tooling scripts are removed.
