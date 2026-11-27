# Documentation site

The documentation site is built with [VitePress](https://vitepress.dev) from the Markdown files in this repository, and published to GitHub Pages by [`.github/workflows/docs.yml`](../../.github/workflows/docs.yml). The Markdown files are the single source: they read directly on GitHub and in an editor, render as static HTML for human readers, and export clean Markdown mirrors and `llms*.txt` indices at build time for external AI agents fetching site URLs.

## Layout

| Path | Contents |
|---|---|
| `docs/**/*.md` | Pages. `docs/index.md` serves as `/`; `docs/explain/index.md` serves as `/explain/`. `docs/README.md` is the repository task index, not a site page. |
| `conformance/semantics.md`, `ROADMAP.md`, `RELEASING.md`, `CONTRIBUTING.md` | Shared documents, also rendered as pages (`SHARED_PAGES` in `docs/.vitepress/llms.ts`) |
| `docs/.vitepress/config.ts` | Site configuration, navigation, link handling, per-page metadata, sitemap and `buildEnd` export hook |
| `docs/.vitepress/llms.ts` | Single authority for site page discovery (`SHARED_PAGES`), repository link resolution, per-page metadata, and build-time `.md` mirrors and `llms*.txt` bundles, with `llms.test.ts` |
| `docs/.vitepress/comparisonMarkdown.ts` | Build-time `comparison.json` snapshot loader and Markdown renderer, with `comparisonMarkdown.test.ts` |
| `docs/.vitepress/theme/components/` | Vue components for figures, benchmark views (`ImplementationComparison.vue`, `PerformanceHistory.vue`), build-time data loaders (`comparisonSnapshot.data.ts`, `implementationChoices.data.ts`) and `PageMarkdownBar.vue` |
| `docs/.vitepress/theme/sim/` | The figures' models and engine replays, with their tests |
| `rust/tessera-trace/` | The engine's figure entry points, built for WebAssembly into `theme/sim/tessera_trace.wasm` (not in Git) |
| `docs/public/` | Static files copied verbatim into the site root, including `logo.svg` (a copy of `assets/logo.svg`) and `robots.txt` |
| `package.json`, `bun.lock` | The site's pinned toolchain, at the repository root |

## Commands

Run these from the repository root with the Bun version pinned in `package.json`. The package manifest sits at the root so that pages outside `docs/`, such as `ROADMAP.md`, resolve the same dependencies. Site scripts explicitly use Bun for VitePress and the TypeScript compiler as well as the simulation tests; a separate Node installation is not required.

```sh
bun install --frozen-lockfile  # install the pinned toolchain
bun run docs:wasm              # build the figure engine; dev, test and build run it first
bun run docs:dev               # local server with hot reload
bun run docs:test              # check ownership models, export pipelines and type-checker diagnostics
bun run docs:typecheck         # type-check the config, theme and components
bun run docs:build             # build into docs/.vitepress/dist; fails on dead links
```

`vue-tsc` checks Vue SFC scripts, templates and component props under Bun. Its entry point, `docs/.vitepress/typecheck.ts`, selects the pinned TypeScript 6 JS compiler API (`typescript6`); tooling checks still use native TypeScript 7. The native compiler's Vue content-mapper path is not used: its synchronous API requires Node-private subprocess pipe handles unavailable in Bun. This keeps Node out of the development toolchain without suppressing SFC diagnostics or patching dependencies. `docs:test` verifies that the checker accepts valid components and rejects script, template and component-prop type errors.

The figure engine needs cargo and the WebAssembly target: `rustup target add wasm32-unknown-unknown`. `wasm32` is a documentation build target only, not a supported library platform; the library's platforms are those its [CI lanes](ci.md) test. On `wasm32`, `usize` has 32 bits, so a figure that showed a value the engine derives from `usize` widths, such as the default block size, would show the 32-bit result.

At build time, `comparisonSnapshot.data.ts` loads and validates the published `comparison.json` from the `benchmark-data` branch via `comparisonMarkdown.ts` (returning `null` gracefully when offline so offline builds and tests still succeed). `ImplementationComparison.vue` pre-hydrates from that build-time snapshot so static HTML immediately includes real comparison cards, SVGs, provenance and tables, then refreshes from `benchmark-data` on mount without erasing SSR data if the live fetch fails; the build-time Markdown mirrors and `llms-full.txt` reuse the same snapshot to inline real Markdown tables. To preview unpublished data, generate the file with `writeComparison` from [`comparison.ts`](../../bench/tracking/comparison.ts) over a copy of the branch's journals and either point `DOCS_COMPARISON_FILE` at it or place it under `docs/public/` and pass `VITE_COMPARISON_SOURCE=/<file>`. Delete any temporary file under `docs/public/` afterwards.

Pull requests that touch the site, its pages or the library source run the same checks in `docs.yml`. Pushes to `main` also deploy the site. The repository's Pages source must be set to *GitHub Actions* once, under *Settings → Pages*.

## Static and AI-readable outputs

The build emits machine-readable artifacts alongside the HTML site so external AI agents fetching published URLs receive complete, noise-free documentation without executing client-side JavaScript:

- **Per-page `.md` mirrors:** emitted in `docs/.vitepress/dist/` next to every served page (`index.md`, `guide/getting-started.md`, `conformance/semantics.md`, `benchmarks/implementations.md`, etc.). Each mirror strips frontmatter (expanding the home page's `hero` into Markdown), removes `<script>` blocks, SPDX comments and layout wrapper `<div>` tags, replaces `<ImplementationComparison>` (`headlines`, `overview`, or `all`) with rendered Markdown tables from the build-time snapshot, links `<PerformanceHistory>` to `benchmark-data/tessera`, strips interactive figure tags whose content is already stated in prose, preserves raw LaTeX math, and rewrites relative links to canonical `.md` site URLs or GitHub `blob`/`tree` URLs.
- **Audience-tiered indices:** `/llms.txt` derives every page's title and one-line summary directly from its own Markdown source and groups pages automatically from path rules (`Using Tessera`, `How Tessera Works` ordered by chapter prefix, and internal contributor docs and unimplemented proposals isolated under `## Optional`); `/llms-full.txt` bundles only the external-user core (`Using Tessera` plus the explanation overview), excluding `evidence.md` and unimplemented proposals; `/llms-explain.txt` bundles the explanation overview and chapters in reading order.
- **Discovery metadata and UI:** `config.ts` generates `sitemap.xml` and injects per-page `<meta name="description">`, canonical URLs, Open Graph title/description, and `<link rel="alternate" type="text/markdown">` tags. `docs/public/robots.txt` advertises `sitemap.xml`. `PageMarkdownBar.vue` adds compact Copy Markdown, View Markdown and `llms.txt` controls to documentation pages.

## Embed the tracker

To embed `PerformanceHistory` in another Vue-enabled Markdown page:

```html
<script setup>
import PerformanceHistory from "./path/to/PerformanceHistory.vue";
</script>
<PerformanceHistory />
```

Optional `source` (a data-directory URL ending in `/`) and `case-id` props select another compatible dataset or the initially compared case; a URL hash takes precedence. Other webpages can use an iframe. GitHub Markdown uses the [README's fixed SVG preview](../../README.md) of the group means. Publishing measurements does not rebuild this website.

## Writing pages

Site pages serve human readers—someone deciding whether to use Tessera (home and performance), using it (guide), or learning how it works (explanation)—as well as external AI agents fetching published HTML, `.md` mirrors or `llms*.txt` bundles. Contributors read the repository documents.

- One idea per paragraph, conclusion first. Explain a technique as the problem, the idea, why it is correct and what it saves, with a small example or number.
- Reader pages name no functions or source files; the [engine reference](../design/engine.md) maps rules to code. Release status and migration stay at the end of the guide.
- State results, not their history: no decisions, experiments or process.

- Write plain Markdown with relative links, as for GitHub. The site turns a link to another page into a site link, and a link to source code, a directory or a file outside the site into a GitHub link.
- A figure is a Vue component placed on its own line, for example `<BlockPartitionDemo />`. Register new components in `docs/.vitepress/theme/index.ts`.
- The text around a figure must say everything the figure shows. Readers of the Markdown source, including agents, do not see the figures.
- A figure that shows an algorithm computes its frames rather than scripting them. Where the engine exposes the stage, the figure replays the engine itself: the library's internal `trace` feature reports each step (`rust/tessera-order/src/engine/trace.rs`), `rust/tessera-trace` exports it to WebAssembly, and `theme/sim/engine.ts` decodes it. The replay applies events and describes them; it decides nothing the engine decides. The block partition works this way. Other figures still run TypeScript models that follow the Rust source's index arithmetic and name the functions they follow; when the engine's rules change, update such a model and its page in the same change. Test replays and models that move records for stable output and one live owner in every frame.
- A step-through figure navigates by the algorithm's own structure, not a uniform frame slider: steps differ in weight (one record against a whole ledger), so a frame index says nothing about progress. Show the phases at the top with the caption under them, step with one control and the arrow keys, and let the figure's own elements jump, as the block partition's input positions and ledger blocks do. Show the logical structure (a bucket's chain) next to the physical layout, and label input positions with their index.
- Formulas use `$…$` and `$$…$$` (MathJax).
- New source files start with an SPDX header, as everywhere in the repository.

## Ownership

[`docs/README.md`](../README.md#who-owns-what) maps each fact to its single owner. `docs/explain/` owns the algorithm's rules, thresholds and reasons, in reading order for anyone, and its figures run models of the Rust source. The [engine reference](../design/engine.md) owns boundaries and invariants and states no threshold. A page that needs a number it does not own links to the owner, so a rule change edits the code, one explanation page and its model.
