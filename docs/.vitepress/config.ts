// SPDX-License-Identifier: BSD-2-Clause
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs'
import { dirname, join, relative, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'
import { defineConfig, type MarkdownRenderer } from 'vitepress'

// The site renders the Markdown files of the repository in place, so the
// same files read correctly on GitHub, in an editor, and on the site. Pages
// are the files under `docs/` plus the shared documents listed here.
const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..')
const repository = 'https://github.com/Yikai-Liao/tessera-sort'
const sharedPages = new Set([
  'conformance/semantics.md',
  'ROADMAP.md',
  'RELEASING.md',
  'CONTRIBUTING.md',
  'CHANGELOG.md',
])
// `docs/README.md` indexes the directory for GitHub readers; the site's home
// page and sidebar replace it.
const notPages = new Set(['docs/README.md'])

function isPage(path: string): boolean {
  if (!path.endsWith('.md') || notPages.has(path)) return false
  return sharedPages.has(path) || (path.startsWith('docs/') && !path.startsWith('docs/.vitepress/'))
}

/** Where a source file is served: `docs/` is the site root, `README.md` is a directory index. */
function sitePath(path: string): string {
  const served = path.startsWith('docs/') ? path.slice('docs/'.length) : path
  return served.replace(/(^|\/)README\.md$/, '$1index.md')
}

function markdownFiles(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    if (entry === 'node_modules' || entry === 'target' || entry.startsWith('.git')) continue
    const full = join(dir, entry)
    const stat = statSync(full)
    if (stat.isDirectory()) {
      if (entry === '.vitepress') continue
      markdownFiles(full, out)
    } else if (entry.endsWith('.md')) {
      out.push(relative(repoRoot, full).split(sep).join('/'))
    }
  }
  return out
}

const allMarkdown = markdownFiles(repoRoot)
const rewrites = Object.fromEntries(
  allMarkdown.filter((p) => isPage(p) && sitePath(p) !== p).map((p) => [p, sitePath(p)]),
)

/**
 * Turns relative links into what each reader needs: a page link becomes an
 * absolute site path, so rewrites cannot break it, and a link to source code,
 * a directory or a file outside the site goes to GitHub.
 */
function repositoryLinks(md: MarkdownRenderer) {
  md.core.ruler.push('tessera_repository_links', (state) => {
    const source: string | undefined = state.env.realPath ?? state.env.path
    if (!source) return
    const visit = (tokens: typeof state.tokens) => {
      for (const token of tokens) {
        if (token.children) visit(token.children)
        if (token.type !== 'link_open') continue
        const href = token.attrGet('href')
        if (!href || /^[a-z][a-z0-9+.-]*:/i.test(href) || href.startsWith('#') || href.startsWith('/')) continue
        const [path, hash = ''] = href.split('#', 2)
        const target = resolve(dirname(source), decodeURIComponent(path))
        const rel = relative(repoRoot, target).split(sep).join('/')
        if (rel.startsWith('..')) continue
        const anchor = hash ? `#${hash}` : ''
        if (isPage(rel) && existsSync(target)) {
          token.attrSet('href', `/${sitePath(rel)}${anchor}`)
        } else if (existsSync(target)) {
          const kind = statSync(target).isDirectory() ? 'tree' : 'blob'
          token.attrSet('href', `${repository}/${kind}/main/${rel}${anchor}`)
        }
        // A missing target is left alone, so the dead-link check reports it.
      }
    }
    visit(state.tokens)
  })
}

export default defineConfig({
  title: 'Tessera',
  description: 'Stable radix sort with O(√n) auxiliary memory',
  lang: 'en-US',
  // GitHub Pages serves the project under /<repository>/; the workflow sets it.
  base: process.env.DOCS_BASE ?? '/',
  srcDir: '..',
  srcExclude: allMarkdown.filter((p) => !isPage(p)),
  rewrites,
  cleanUrls: true,
  lastUpdated: true,
  head: [['link', { rel: 'icon', type: 'image/svg+xml', href: `${process.env.DOCS_BASE ?? '/'}logo.svg` }]],
  markdown: {
    math: true,
    config: repositoryLinks,
  },
  vite: {
    // `srcDir` is the repository root, one level above this Vite project.
    // Vite would otherwise look for static assets in `<srcDir>/public`.
    publicDir: resolve(dirname(fileURLToPath(import.meta.url)), '../public'),
    server: { fs: { allow: [repoRoot] } },
  },
  themeConfig: {
    logo: '/logo.svg',
    nav: [
      { text: 'Guide', link: '/guide/getting-started', activeMatch: '^/(guide|conformance)/' },
      { text: 'How it works', link: '/explain/', activeMatch: '^/explain/' },
      {
        text: 'Development',
        link: '/design/engine',
        activeMatch: '^/(design|development|decisions|proposals)/|^/(CONTRIBUTING|RELEASING|ROADMAP|CHANGELOG)',
      },
    ],
    sidebar: {
      '/guide/': guideSidebar(),
      '/conformance/': guideSidebar(),
      '/explain/': [
        {
          text: 'How Tessera works',
          items: [
            { text: 'Overview', link: '/explain/' },
            { text: '1. Radix sort in one pass', link: '/explain/radix-sort' },
            { text: '2. Keys as unsigned integers', link: '/explain/keys' },
            { text: '3. Routing an input', link: '/explain/routing' },
            { text: '4. The block partition', link: '/explain/block-partition' },
            { text: '5. Finishing tiles', link: '/explain/tiles' },
            { text: '6. Ownership and panics', link: '/explain/safety' },
          ],
        },
      ],
      '/': developmentSidebar(),
    },
    outline: { level: [2, 3] },
    search: { provider: 'local' },
    editLink: {
      // A string, not a function: VitePress serializes this option for the client.
      pattern: `${repository}/edit/main/:path`,
      text: 'Edit this page on GitHub',
    },
    socialLinks: [{ icon: 'github', link: repository }],
    footer: {
      message: 'Released under the BSD 2-Clause License.',
    },
  },
})

function guideSidebar() {
  return [
    {
      text: 'User guide',
      items: [
        { text: 'Getting started', link: '/guide/getting-started' },
        { text: 'Reusing scratch memory', link: '/guide/workspace' },
        { text: 'Ordering semantics', link: '/conformance/semantics' },
      ],
    },
  ]
}

function firstHeading(path: string): string {
  const heading = readFileSync(join(repoRoot, path), 'utf8').match(/^# (.+)$/m)
  return heading ? heading[1] : path
}

function developmentSidebar() {
  const decisions = allMarkdown
    .filter((p) => /^docs\/decisions\/\d{4}-.*\.md$/.test(p))
    .sort()
    .map((p) => ({ text: firstHeading(p), link: `/${sitePath(p).replace(/\.md$/, '')}` }))
  return [
    {
      text: 'Design',
      items: [
        { text: 'Engine design', link: '/design/engine' },
        { text: 'Port from the prototype', link: '/design/port' },
      ],
    },
    {
      text: 'Development',
      items: [
        { text: 'Contributing', link: '/CONTRIBUTING' },
        { text: 'Benchmark protocol', link: '/development/benchmarking' },
        { text: 'Continuous benchmarks', link: '/development/continuous-benchmarks' },
        { text: 'Documentation site', link: '/development/docs-site' },
        { text: 'Releasing', link: '/RELEASING' },
      ],
    },
    {
      text: 'Planning',
      items: [
        { text: 'Roadmap', link: '/ROADMAP' },
        { text: 'Changelog', link: '/CHANGELOG' },
        { text: 'Python binding proposal', link: '/proposals/python-binding' },
      ],
    },
    {
      text: 'Decision records',
      collapsed: true,
      items: [{ text: 'Index', link: '/decisions/' }, ...decisions],
    },
  ]
}
