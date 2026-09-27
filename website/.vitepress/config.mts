import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { slug } from 'github-slugger'
import { defineConfigWithTheme, type DefaultTheme, type HeadConfig } from 'vitepress'
import { lastCommitMs, repoLinks, scanSite } from './repo-links'

const srcDir = fileURLToPath(new URL('..', import.meta.url))
const repoRoot = path.resolve(srcDir, '..')

const base = '/stretto/'
// `vitepress build` sets NODE_ENV before it reads this file; `vitepress dev` does not.
const isBuild = process.env.NODE_ENV === 'production'
const siteUrl = 'https://alexnodeland.github.io/stretto/'
const repoUrl = 'https://github.com/alexnodeland/stretto'
const branch = 'main'

const title = 'stretto'
const description =
  'stretto learns, from an LLM agent’s recorded tool calls, which reads it makes next and where their arguments come from, and serves those reads through an MCP proxy, so the agent needs fewer LLM turns.'

// Which repository files the site renders, and on which page (see repo-links.ts).
const site = scanSite(srcDir, repoRoot)

/**
 * Brand media the site embeds once they are copied into public/ (the brand kit is
 * made separately). A slot whose file is missing renders nothing in a build, and a
 * hint in `npm run dev`. See theme/components/BrandEmbed.vue.
 */
const brandFiles = {
  explainer: 'explainer/index.html',
  launchVideo: 'media/launch.mp4',
  launchPoster: 'media/launch-poster.png',
  walkthroughVideo: 'media/walkthrough.mp4',
  walkthroughPoster: 'media/walkthrough-poster.png',
  launchCaptions: 'media/launch.vtt',
  walkthroughCaptions: 'media/walkthrough.vtt'
} as const

export type BrandAssets = Record<keyof typeof brandFiles, string | false>

const brandAssets = Object.fromEntries(
  Object.entries(brandFiles).map(([key, file]) => [
    key,
    fs.existsSync(path.join(srcDir, 'public', file)) ? '/' + file : false
  ])
) as BrandAssets

export interface ThemeConfig extends DefaultTheme.Config {
  brandAssets: BrandAssets
}

const guide: DefaultTheme.SidebarItem[] = [
  {
    text: 'Introduction',
    items: [
      { text: 'What is stretto?', link: '/guide/' },
      { text: 'Why stretto?', link: '/guide/why' },
      { text: 'Quick start', link: '/guide/quick-start' },
      { text: 'Installation', link: '/guide/installation' },
      { text: 'How it works', link: '/guide/how-it-works' }
    ]
  },
  {
    text: 'Core concepts',
    items: [
      { text: 'Sessions and recording', link: '/guide/concepts/sessions' },
      { text: 'Flows', link: '/guide/concepts/flows' },
      { text: 'Lookups and detours', link: '/guide/concepts/lookups' },
      { text: 'Deciders: reach, habit, arbiter', link: '/guide/concepts/deciders' },
      { text: 'Bindings', link: '/guide/concepts/bindings' },
      { text: 'Shadow mode and promotion', link: '/guide/concepts/shadow-and-promotion' },
      { text: 'Audit and review', link: '/guide/concepts/audit-and-review' },
      { text: 'Procedures', link: '/guide/concepts/procedures' },
      { text: 'Privacy and redaction', link: '/guide/concepts/privacy' }
    ]
  },
  {
    text: 'Tutorials',
    items: [{ text: 'Your own MCP server, end to end', link: '/guide/walkthrough' }]
  }
]

const integrations: DefaultTheme.SidebarItem[] = [
  {
    text: 'Integrations',
    items: [
      { text: 'Any MCP host', link: '/integrations/' },
      { text: 'Claude Code', link: '/integrations/claude-code' },
      { text: 'Claude Desktop', link: '/integrations/claude-desktop' },
      { text: 'Cursor and VS Code', link: '/integrations/cursor-vscode' },
      { text: 'Streamable HTTP servers', link: '/integrations/streamable-http' }
    ]
  }
]

const reference: DefaultTheme.SidebarItem[] = [
  {
    text: 'Reference',
    items: [
      { text: 'Command line', link: '/reference/cli' },
      { text: 'stretto-proxy', link: '/reference/proxy' },
      { text: 'File formats', link: '/reference/formats' },
      { text: 'Environment variables', link: '/reference/environment' }
    ]
  },
  {
    text: 'In depth',
    items: [
      { text: 'Reviewing flows', link: '/reference/review' },
      { text: 'Privacy: files and data', link: '/reference/privacy' },
      { text: 'Design summary', link: '/reference/design' }
    ]
  }
]

const research: DefaultTheme.SidebarItem[] = [
  {
    text: 'Research',
    items: [
      { text: 'Overview', link: '/research/' },
      { text: 'The paper', link: '/research/paper' },
      { text: 'Claims and evidence', link: '/research/claims' },
      { text: 'Results index', link: '/research/results' },
      { text: 'Seven benchmarks', link: '/research/benchmarks' },
      { text: 'Frontier models, live', link: '/research/frontier' },
      { text: 'Live on AgentDojo and BFCL', link: '/research/live' },
      { text: 'Reproduce the results', link: '/research/reproduce' },
      { text: 'Research notebook', link: '/notebook/', target: '_self' }
    ]
  }
]

const community: DefaultTheme.SidebarItem[] = [
  {
    text: 'Community',
    items: [
      { text: 'Contributing', link: '/community/contributing' },
      { text: 'Changelog', link: '/community/changelog' },
      { text: 'Roadmap', link: '/community/roadmap' },
      { text: 'FAQ', link: '/community/faq' },
      { text: 'License', link: '/community/license' }
    ]
  }
]

export default defineConfigWithTheme<ThemeConfig>({
  title,
  description,
  lang: 'en-US',
  base,
  cleanUrls: true,
  lastUpdated: true,

  sitemap: {
    hostname: siteUrl,
    // The research notebook is copied in as a static page (scripts/copy-notebook.mjs).
    transformItems: (items) => [...items, { url: 'notebook/' }]
  },

  head: [
    ['link', { rel: 'icon', href: `${base}favicon.ico`, sizes: '48x48' }],
    ['link', { rel: 'icon', type: 'image/svg+xml', href: `${base}favicon.svg` }],
    ['link', { rel: 'apple-touch-icon', href: `${base}apple-touch-icon.png` }],
    ['link', { rel: 'manifest', href: `${base}site.webmanifest` }],
    ['meta', { name: 'theme-color', content: '#f8fbfb', media: '(prefers-color-scheme: light)' }],
    ['meta', { name: 'theme-color', content: '#0b0f11', media: '(prefers-color-scheme: dark)' }],
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:site_name', content: title }],
    ['meta', { property: 'og:image', content: `${siteUrl}og.png` }],
    ['meta', { property: 'og:image:width', content: '1200' }],
    ['meta', { property: 'og:image:height', content: '630' }],
    ['meta', { property: 'og:image:alt', content: 'stretto: Read ahead of your agent.' }],
    ['meta', { name: 'twitter:card', content: 'summary_large_image' }],
    ['meta', { name: 'twitter:image', content: `${siteUrl}og.png` }]
  ],

  // Each page's own title, description and address, for link previews.
  transformHead({ pageData, siteData }) {
    if (pageData.isNotFound || pageData.relativePath === '404.md') return []
    const route = pageData.relativePath.replace(/(^|\/)index\.md$/, '$1').replace(/\.md$/, '')
    const url = siteUrl + route
    const pageTitle = pageData.title && pageData.title !== siteData.title ? `${pageData.title} | ${siteData.title}` : siteData.title
    const pageDescription = pageData.description || siteData.description
    const head: HeadConfig[] = [
      ['link', { rel: 'canonical', href: url }],
      ['meta', { property: 'og:url', content: url }],
      ['meta', { property: 'og:title', content: pageTitle }],
      ['meta', { property: 'og:description', content: pageDescription }],
      ['meta', { name: 'twitter:title', content: pageTitle }],
      ['meta', { name: 'twitter:description', content: pageDescription }]
    ]
    return head
  },

  // A page that includes a repository file is edited, and dated, at that file.
  transformPageData(pageData) {
    const sources = site.sources.get(pageData.filePath)
    if (!sources?.length) return
    const files = [path.join(srcDir, pageData.filePath), ...sources.map((file) => path.join(repoRoot, file))]
    const updated = lastCommitMs(files, repoRoot)
    return {
      frontmatter: { editSource: sources[0], ...pageData.frontmatter },
      ...(updated && pageData.frontmatter.lastUpdated !== false ? { lastUpdated: updated } : {})
    }
  },

  markdown: {
    math: true,
    // GitHub's heading anchors, so that a #fragment written for the repository works here.
    anchor: { slugify: slug },
    image: { lazyLoading: true },
    config(md) {
      repoLinks(md, {
        repoRoot,
        srcDir,
        base,
        repoUrl,
        branch,
        pages: site.pages,
        // The research notebook was the whole site before; it now lives at /notebook/,
        // copied in as a static page (scripts/copy-notebook.mjs).
        moved: { 'https://alexnodeland.github.io/stretto': '/notebook/' },
        staticPaths: ['/notebook/'],
        devServer: !isBuild
      })

      // Short inline code (an option, a command, a field) is not broken across lines,
      // such as `--` at a hyphen; longer code still wraps.
      const codeInline = md.renderer.rules.code_inline!
      md.renderer.rules.code_inline = (tokens, idx, options, env, self) => {
        if (tokens[idx].content.length <= 24) tokens[idx].attrJoin('class', 'nobr')
        return codeInline(tokens, idx, options, env, self)
      }

      // ```mermaid blocks render as diagrams, in the browser.
      const fence = md.renderer.rules.fence!
      md.renderer.rules.fence = (tokens, idx, options, env, self) => {
        const token = tokens[idx]
        if (token.info.trim() === 'mermaid') {
          return `<MermaidDiagram code="${encodeURIComponent(token.content)}" />\n`
        }
        return fence(tokens, idx, options, env, self)
      }
    }
  },

  themeConfig: {
    logo: { light: '/logo.svg', dark: '/logo-dark.svg', alt: '' },
    siteTitle: 'stretto',

    nav: [
      { text: 'Guide', link: '/guide/', activeMatch: '^/guide/' },
      { text: 'Integrations', link: '/integrations/', activeMatch: '^/integrations/' },
      { text: 'Reference', link: '/reference/cli', activeMatch: '^/reference/' },
      { text: 'Research', link: '/research/', activeMatch: '^/research/' },
      {
        text: 'Community',
        activeMatch: '^/community/',
        items: [
          { text: 'Contributing', link: '/community/contributing' },
          { text: 'Changelog', link: '/community/changelog' },
          { text: 'Roadmap', link: '/community/roadmap' },
          { text: 'FAQ', link: '/community/faq' },
          { text: 'License', link: '/community/license' },
          { text: 'Issues', link: `${repoUrl}/issues` }
        ]
      }
    ],

    sidebar: {
      '/guide/': guide,
      '/integrations/': integrations,
      '/reference/': reference,
      '/research/': research,
      '/community/': community
    },

    socialLinks: [{ icon: 'github', link: repoUrl }],

    editLink: {
      // `https://github.com/alexnodeland/stretto/edit/main/website/:path`, except that a
      // page made from a repository file (front matter `editSource`) edits that file.
      // VitePress sends this function to the browser as text, so it names no variable.
      pattern: ({ filePath, frontmatter }) =>
        frontmatter.editSource
          ? `https://github.com/alexnodeland/stretto/edit/main/${frontmatter.editSource}`
          : `https://github.com/alexnodeland/stretto/edit/main/website/${filePath}`,
      text: 'Edit this page on GitHub'
    },

    lastUpdated: {
      text: 'Last updated',
      formatOptions: { dateStyle: 'medium' }
    },

    footer: {
      message: 'Released under the MIT License.',
      copyright: 'Copyright © 2026 Alex Nodeland'
    },

    search: { provider: 'local' },
    outline: [2, 3],
    externalLinkIcon: true,

    brandAssets
  },

  vite: {
    server: {
      // Included Markdown and its images live outside website/.
      fs: { allow: [repoRoot] }
    },
    build: {
      // Mermaid's chunks are large, and load only on pages with a diagram.
      chunkSizeWarningLimit: 1600
    }
  }
})
