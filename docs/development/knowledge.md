---
type: Playbook
title: Knowledge workflow
description: Retrieve task context with OKF and maintain a small current corpus.
tags: [docs, agent]
---

<!-- SPDX-License-Identifier: BSD-2-Clause -->

# Knowledge workflow

The goal is a small, current corpus that an agent can enter by task, inspect by description and read only as needed. [AGENTS.md](../../AGENTS.md) carries universal safety rules and task routing; [the task index](../index.md) is the no-tool fallback. Markdown is the only maintained content.

## Retrieve progressively

Install the [OKF CLI](https://github.com/okfcli/okf) release **v0.5.2**, then build the local bundle:

```sh
pnpm knowledge:build
# CLI command/flag discovery, without reading its manual:
okf schema search
# Find candidate IDs, not full bodies:
okf search .knowledge --tag engine --type Reference
okf search .knowledge --text "panic"
# Inspect a directory index for descriptions; then read one selected topic:
# .knowledge/docs/design/index.md
okf show .knowledge docs/design/engine
# Follow relevant dependencies, not the entire graph:
okf backlinks .knowledge conformance/semantics
```

Search is case-insensitive literal matching, with AND-combined filters, not semantic ranking. Search returns IDs/titles/types; generated directory indexes include descriptions. `show` returns the selected body under `concept.body`. Use `okf list` for inventory and `okf graph` for maintenance, not as routine context. Prefer the task entry when you already know the task; use tags before broad text search. Stop when you have the applicable contract, procedure and evidence.

The bundle mirrors Git-tracked Markdown plus new nonignored Markdown, excluding `.github/` platform templates and duplicate symlink aliases. Platform templates keep their GitHub-required form and are not knowledge concepts. Rebuild after edits. It is disposable and ignored: no embeddings, model calls, network access or generated content in Git. It excludes dependencies, benchmark results, vendored sources and site output. Do not run `okf index .`: it would scan generated/vendor Markdown and overwrite authored `index.md` files.

## Maintain the corpus

Before adding a document, ask whether the task needs a new contract, procedure, explanation or evidence that cannot fit its existing topic. Edit that topic first. Delete obsolete instructions; do not retain them as active historical references. Git/PR history preserves the old decision. Keep non-obvious rationale and reproducible measurement conditions beside the current choice.

Every maintained non-index Markdown document outside `.github/` starts with OKF frontmatter:

```yaml
---
type: Playbook
title: Short task name
description: When to read this and what task it enables.
tags: [bench, performance]
---
```

- Types: `Reference` for current contracts/design, `Playbook` for procedures, `Explanation` for teaching, `Proposal` for unimplemented plans, `Evidence` for measured decisions, `Policy` for contribution/security rules. Use existing types/tags before adding vocabulary.
- Descriptions are a single task-selecting sentence, not a generic summary. Tags connect tasks across directories: `engine`, `safety`, `performance`, `bench`, `ci`, `docs`, `release`, `api`, `python`. Add narrower tags only when they help distinguish a topic.
- Put the usable contract or action first. Link prerequisites and evidence; do not repeat rules, commands, case inventories or configuration owned elsewhere. Keep exact algorithm constants and failure guarantees where needed. Markdown paragraphs occupy one line.
- `status: draft` marks proposals not yet implemented; stable is the OKF default, not a verification claim. `sources` may identify code or evidence. Add `generated`, `verified` or `stale_after` only with real attribution, verification or expiry; a successful schema check does not verify prose.
- Root `index.md` declares `okf_version: "0.2"` in the generated bundle; other indexes are navigation only. Authored concepts cannot use OKF's reserved `index.md`/`log.md` names. Site landing concepts are `home.md` and `explain/overview.md`; public URLs are unchanged.
- Run `pnpm knowledge:test` and `pnpm knowledge:build`; run site checks for pages and models. Update retrieval examples/tests if a topic moves. Generated indexes need no manual edits.

## Compatibility and limits

The [Google OKF specification](https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md) and CLI have separate versions. This repository targets **OKF 0.2** and tests **CLI 0.5.2**; upgrade deliberately by checking metadata, generated indexes and representative searches together. Custom type names are permitted by OKF. No custom ranking or schema registry is maintained.

`tools/knowledge.ts` only selects Markdown, copies it without rewriting and invokes `okf validate`/`okf index`. Missing metadata fails the build. Retrieval tests check engine contracts, current-head feedback, corpus maintenance and performance evidence; they also reject Markdown-link warnings while tolerating the CLI's non-concept source-file warnings. The CLI treats source-code links as concept links and can warn about valid non-Markdown files absent from the bundle; validation does not certify links or prose. The site build checks page links. Markdown outside the site still needs review. Source code comments/API doctests remain with code, not copied into the corpus.
