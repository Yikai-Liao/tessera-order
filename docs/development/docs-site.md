---
type: Playbook
title: Documentation site
description: Edit site pages, interactive figures or algorithm simulation models.
tags: [docs, site]
---

# Documentation site

The documentation site is built with [VitePress](https://vitepress.dev) from the Markdown files in this repository, and published to GitHub Pages by [`.github/workflows/docs.yml`](../../.github/workflows/docs.yml). The Markdown files are the source: they read correctly on GitHub and in an editor, and coding agents can read them directly.

## Layout

| Path | Contents |
|---|---|
| `docs/**/*.md` | Pages. `docs/home.md` serves as `/`; `docs/explain/overview.md` serves as `/explain/`. `docs/index.md` is the repository task index, not a site page. The reserved `index.md` name is kept for OKF navigation. |
| `conformance/semantics.md`, `ROADMAP.md`, `RELEASING.md`, `CONTRIBUTING.md` | Shared documents, also rendered as pages (`sharedPages` in the config) |
| `docs/.vitepress/config.ts` | Site configuration, navigation and link handling |
| `docs/.vitepress/theme/components/` | Vue components used by the explanation pages |
| `docs/.vitepress/theme/sim/` | TypeScript models of the engine that drive the figures, with their tests |
| `docs/public/` | Static files, such as the logo (a copy of `assets/logo.svg`) |
| `package.json`, `pnpm-lock.yaml` | The site's pinned toolchain, at the repository root |

## Commands

Run these from the repository root, with Node.js 24 or later. The pnpm project sits at the root so that pages outside `docs/`, such as `ROADMAP.md`, resolve the same dependencies.

```sh
pnpm install --frozen-lockfile  # install the pinned toolchain
pnpm docs:dev                   # local server with hot reload
pnpm docs:test                  # check the models against reference sorts
pnpm docs:typecheck             # type-check the config, theme and components
pnpm docs:build                 # build into docs/.vitepress/dist; fails on dead links
```

The pinned TypeScript compiler and Vue content mapper check SFC scripts and templates; `--runExternalCode` enables the mapper registered in `docs/.vitepress/tsconfig.json`.

Pull requests that touch the site or its pages run the same checks in `docs.yml`. Pushes to `main` also deploy the site. The repository's Pages source must be set to *GitHub Actions* once, under *Settings → Pages*.

## Writing pages

- Write plain Markdown with relative links, as for GitHub. The site turns a link to another page into a site link, and a link to source code, a directory or a file outside the site into a GitHub link.
- A figure is a Vue component placed on its own line, for example `<BlockPartitionDemo />`. Register new components in `docs/.vitepress/theme/index.ts`.
- The text around a figure must say everything the figure shows. Readers of the Markdown source, including agents, do not see the figures.
- A figure that shows an algorithm runs a model in `theme/sim/` rather than scripted frames. The model follows the Rust source's index arithmetic, names the functions it follows, and has a test that compares it against a reference. When the engine changes, update the model and its page in the same change.
- Formulas use `$…$` and `$$…$$` (MathJax).
- New source files start with an SPDX header, as everywhere in the repository.

## Explanation pages

`docs/explain/` teaches how the algorithm works, in reading order, for anyone. It does not replace the [engine design](../design/engine.md), which states the engine's decisions and constants for readers of the source. A change to a decision updates both.
