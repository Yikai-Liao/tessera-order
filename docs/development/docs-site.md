# Documentation site

The documentation site is built with [VitePress](https://vitepress.dev) from the Markdown files in this repository, and published to GitHub Pages by [`.github/workflows/docs.yml`](../../.github/workflows/docs.yml). The Markdown files are the source: they read correctly on GitHub and in an editor, and coding agents can read them directly.

## Layout

| Path | Contents |
|---|---|
| `docs/**/*.md` | Pages. `docs/index.md` serves as `/`; `docs/explain/index.md` serves as `/explain/`. `docs/README.md` is the repository task index, not a site page. |
| `conformance/semantics.md`, `ROADMAP.md`, `RELEASING.md`, `CONTRIBUTING.md` | Shared documents, also rendered as pages (`sharedPages` in the config) |
| `docs/.vitepress/config.ts` | Site configuration, navigation and link handling |
| `docs/.vitepress/theme/components/` | Vue components used by the explanation pages |
| `docs/.vitepress/theme/sim/` | TypeScript models of the engine that drive the figures, with their tests |
| `docs/public/` | Static files, such as the logo (a copy of `assets/logo.svg`) |
| `package.json`, `bun.lock` | The site's pinned toolchain, at the repository root |

## Commands

Run these from the repository root with the Bun version pinned in `package.json`. The package manifest sits at the root so that pages outside `docs/`, such as `ROADMAP.md`, resolve the same dependencies. Site scripts explicitly use Bun for VitePress and the TypeScript compiler as well as the simulation tests; a separate Node installation is not required.

```sh
bun install --frozen-lockfile  # install the pinned toolchain
bun run docs:dev               # local server with hot reload
bun run docs:test              # check ownership models and type-checker diagnostics
bun run docs:typecheck         # type-check the config, theme and components
bun run docs:build             # build into docs/.vitepress/dist; fails on dead links
```

`vue-tsc` checks Vue SFC scripts, templates and component props under Bun. Its entry point, `docs/.vitepress/typecheck.ts`, selects the pinned TypeScript 6 JS compiler API (`typescript6`); tooling checks still use native TypeScript 7. The native compiler's Vue content-mapper path is not used: its synchronous API requires Node-private subprocess pipe handles unavailable in Bun. This keeps Node out of the development toolchain without suppressing SFC diagnostics or patching dependencies. `docs:test` verifies that the checker accepts valid components and rejects script, template and component-prop type errors.

Pull requests that touch the site or its pages run the same checks in `docs.yml`. Pushes to `main` also deploy the site. The repository's Pages source must be set to *GitHub Actions* once, under *Settings → Pages*.

## Writing pages

- Write plain Markdown with relative links, as for GitHub. The site turns a link to another page into a site link, and a link to source code, a directory or a file outside the site into a GitHub link.
- A figure is a Vue component placed on its own line, for example `<BlockPartitionDemo />`. Register new components in `docs/.vitepress/theme/index.ts`.
- The text around a figure must say everything the figure shows. Readers of the Markdown source, including agents, do not see the figures.
- A figure that shows an algorithm runs a model in `theme/sim/` rather than scripted frames. The model follows the Rust source's index arithmetic and names the functions it follows. Focus reference tests on models that move records: check stable output and one live owner throughout partitioning and relocation. When the engine's rules change, update the model and its page in the same change.
- Formulas use `$…$` and `$$…$$` (MathJax).
- New source files start with an SPDX header, as everywhere in the repository.

## Ownership

[`docs/README.md`](../README.md#who-owns-what) maps each fact to its single owner. `docs/explain/` owns the algorithm's rules, thresholds and reasons, in reading order for anyone, and its figures run models of the Rust source. The [engine reference](../design/engine.md) owns boundaries and invariants and states no threshold. A page that needs a number it does not own links to the owner, so a rule change edits the code, one explanation page and its model.
