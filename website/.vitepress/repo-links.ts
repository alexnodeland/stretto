// Links between the site and the repository.
//
// The site includes the repository's own Markdown (`<!--@include: ../../docs/cli.md-->`),
// and that Markdown links to paths relative to where it lives in the repository
// (`../crates/stretto-proxy/README.md`, `results/pilot-2026-09-24.md`). VitePress
// inlines an included file before parsing, so those links would resolve against the
// including page and break. This plugin finds which file each link came from, then:
//
// - a link to a repository file that the site renders (it is included whole by a page)
//   points at that page, keeping its #fragment;
// - a link to any other repository file or folder points at it on GitHub;
// - an image stored outside `website/` is imported from where it is, so Vite copies it
//   into the build (and `vitepress dev` serves it from there);
// - a link to a file that does not exist fails the build, as VitePress's own dead-link
//   check does for pages.
//
// Headings get GitHub's anchors (see config.mts), so a #fragment written for GitHub
// works on the site too.

import fs from 'node:fs'
import path from 'node:path'
import { spawnSync } from 'node:child_process'
import type { MarkdownRenderer } from 'vitepress'

const INCLUDE_RE = /<!--\s*@include:\s*(.*?)\s*-->/g
const PARTIAL_RE = /(#[\w-]+)|(\{\d*,\d*\})$/
const ABSOLUTE_URL_RE = /^(?:[a-z][a-z\d+.-]*:|\/\/)/i

export interface RepoLinksOptions {
  /** The repository's root directory. */
  repoRoot: string
  /** The site's source directory (`website/`). */
  srcDir: string
  /** The site's base, such as `/stretto/`. */
  base: string
  /** The repository on GitHub, such as `https://github.com/alexnodeland/stretto`. */
  repoUrl: string
  /** The branch the site is built from. */
  branch: string
  /** Repository files the site renders, by repository path, and the route of their page. */
  pages: Map<string, string>
  /** Absolute URLs that now live on this site (without a trailing slash), and their site path. */
  moved?: Record<string, string>
  /**
   * Site paths served as static files, not VitePress pages, such as `/notebook/`. A link
   * to one gets the base and `target="_self"`, so that the router does not take it.
   */
  staticPaths?: string[]
  /** Fail the render when a link points at a file that does not exist (default true). */
  strict?: boolean
  /**
   * Rendering for `vitepress dev`: an image outside the site is served from where it is,
   * through Vite's /@fs/ route. In a build it is imported, and Vite copies it.
   */
  devServer?: boolean
}

export interface SiteSources {
  /** Repository path of each file a page includes whole, and that page's route. */
  pages: Map<string, string>
  /** Page path (relative to `website/`), and the repository files it includes whole. */
  sources: Map<string, string[]>
}

const posix = (p: string) => p.split(path.sep).join('/')

const inside = (file: string, dir: string) => {
  const rel = path.relative(dir, file)
  return rel === '' || (!rel.startsWith('..') && !path.isAbsolute(rel))
}

/** The files a Markdown page includes whole, as absolute paths. */
export function includesOf(page: string): string[] {
  let src: string
  try {
    src = fs.readFileSync(page, 'utf-8')
  } catch {
    return []
  }
  const found: string[] = []
  for (const [, target] of src.matchAll(INCLUDE_RE)) {
    if (!target || target.startsWith('@') || PARTIAL_RE.test(target)) continue
    found.push(path.resolve(path.dirname(page), target))
  }
  return found
}

/** The route VitePress serves a page at, from its path relative to the source directory. */
export function routeOf(pagePath: string): string {
  const route = '/' + posix(pagePath).replace(/\.md$/, '')
  return route.endsWith('/index') ? route.slice(0, -'index'.length) : route
}

/** Every page of the site that includes repository files whole, and where each file renders. */
export function scanSite(srcDir: string, repoRoot: string): SiteSources {
  const pages = new Map<string, string>()
  const sources = new Map<string, string[]>()
  const walk = (dir: string) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      if (entry.name.startsWith('.') || entry.name === 'node_modules' || entry.name === 'public') continue
      const full = path.join(dir, entry.name)
      if (entry.isDirectory()) walk(full)
      else if (entry.name.endsWith('.md')) {
        const included = includesOf(full)
          .filter((file) => inside(file, repoRoot) && !inside(file, srcDir))
          .map((file) => posix(path.relative(repoRoot, file)))
        if (!included.length) continue
        const pagePath = posix(path.relative(srcDir, full))
        sources.set(pagePath, included)
        for (const file of included) {
          if (!pages.has(file)) pages.set(file, routeOf(pagePath))
        }
      }
    }
  }
  walk(srcDir)
  return { pages, sources }
}

/** When git last touched any of these files, in milliseconds since the epoch. */
export function lastCommitMs(files: string[], cwd: string): number | undefined {
  const result = spawnSync('git', ['log', '-1', '--format=%ct', '--', ...files], { cwd, encoding: 'utf-8' })
  const seconds = Number.parseInt(result.stdout?.trim() ?? '', 10)
  return Number.isFinite(seconds) ? seconds * 1000 : undefined
}

let tracked: Set<string> | null | undefined

/** The files and folders git tracks, or null when git cannot say. */
function trackedPaths(repoRoot: string): Set<string> | null {
  if (tracked !== undefined) return tracked
  const result = spawnSync('git', ['ls-files', '-z'], { cwd: repoRoot, encoding: 'utf-8', maxBuffer: 64 << 20 })
  if (result.status !== 0 || !result.stdout) return (tracked = null)
  tracked = new Set<string>()
  for (const file of result.stdout.split('\0')) {
    if (!file) continue
    tracked.add(file)
    for (let dir = path.posix.dirname(file); dir !== '.'; dir = path.posix.dirname(dir)) tracked.add(dir)
  }
  return tracked
}

interface Range {
  start: number
  end: number
  file: string
}

const contentCache = new Map<string, { mtimeMs: number; content: string }>()

/** A file's text as VitePress inlines it: line endings normalized, front matter removed. */
function includedText(file: string): string | undefined {
  try {
    const { mtimeMs } = fs.statSync(file)
    const cached = contentCache.get(file)
    if (cached && cached.mtimeMs === mtimeMs) return cached.content
    let content = fs.readFileSync(file, 'utf-8').replace(/\r\n?/g, '\n').replace(/\0/g, '�')
    if (file.endsWith('.md')) content = content.replace(/^---\n[\s\S]*?\n---(?:\n|$)/, '')
    contentCache.set(file, { mtimeMs, content })
    return content
  } catch {
    return undefined
  }
}

/** The lines of `src` that each included file occupies. */
function locate(src: string, files: string[]): Range[] {
  const ranges: Range[] = []
  for (const file of new Set(files)) {
    const content = includedText(file)
    if (!content) continue
    let from = 0
    for (let at = src.indexOf(content, from); at >= 0; at = src.indexOf(content, from)) {
      const start = src.slice(0, at).split('\n').length - 1
      const lines = content.split('\n').length - (content.endsWith('\n') ? 1 : 0)
      ranges.push({ start, end: start + lines, file })
      from = at + content.length
    }
  }
  return ranges
}

export function repoLinks(md: MarkdownRenderer, options: RepoLinksOptions) {
  const { repoRoot, srcDir, base, repoUrl, branch, pages, moved = {}, staticPaths = [], strict = true, devServer = false } = options
  const shown = (file: string) => posix(path.relative(repoRoot, file))

  md.core.ruler.push('stretto_repo_links', (state) => {
    const env = state.env ?? {}
    const page: string | undefined = env.realPath ?? env.path
    if (!page || !page.endsWith('.md')) return
    // VitePress lists a page's includes in env.includes; its search index renders
    // pages without them, so read them from the page itself then.
    const includes: string[] = env.includes?.length ? env.includes : includesOf(page)
    const ranges = locate(state.src, includes.map((file) => path.resolve(file)))
    const sourceAt = (line: number | undefined) =>
      (line !== undefined && ranges.find((r) => line >= r.start && line < r.end)?.file) || page
    const problems: string[] = []

    /** Where a relative link or image from `source` points: an absolute path and its suffix. */
    const resolve = (target: string, source: string) => {
      const [, file = '', suffix = ''] = target.match(/^([^?#]*)(.*)$/) ?? []
      if (!file) return undefined
      let decoded = file
      try {
        decoded = decodeURIComponent(file)
      } catch {
        // keep it as written
      }
      return { abs: path.resolve(path.dirname(source), decoded), suffix }
    }

    const rewriteLink = (href: string, source: string): { href: string; target?: string } | undefined => {
      if (ABSOLUTE_URL_RE.test(href)) {
        const to = moved[href.replace(/\/+$/, '')]
        if (!to) return undefined
        return staticPaths.includes(to)
          ? { href: (base + to).replace(/\/+/g, '/'), target: '_self' }
          : { href: to }
      }
      if (href.startsWith('/')) {
        // VitePress leaves links that name a target alone, so the base goes on here.
        return staticPaths.includes(href.replace(/[?#].*$/, ''))
          ? { href: (base + href).replace(/\/+/g, '/'), target: '_self' }
          : undefined
      }
      if (href.startsWith('#')) return undefined
      const resolved = resolve(href, source)
      if (!resolved) return undefined
      const { abs, suffix } = resolved
      const hash = suffix.includes('#') ? suffix.slice(suffix.indexOf('#')) : ''
      if (inside(abs, srcDir)) {
        // A page of the site. VitePress resolves links written in the page itself;
        // one from an included file is made absolute, from the site's root.
        return source === page ? undefined : { href: '/' + posix(path.relative(srcDir, abs)) + suffix }
      }
      if (!inside(abs, repoRoot)) {
        problems.push(`${href} (in ${shown(source)}) leaves the repository`)
        return undefined
      }
      const rel = posix(path.relative(repoRoot, abs))
      const route = pages.get(rel)
      if (route) return { href: route + hash }
      if (!fs.existsSync(abs)) {
        problems.push(`${href} (in ${shown(source)}): ${rel} does not exist`)
        return undefined
      }
      const known = trackedPaths(repoRoot)
      if (known && !known.has(rel)) {
        console.warn(`[repo-links] ${href} (in ${shown(source)}): ${rel} is not tracked by git, so GitHub will not have it`)
      }
      const kind = fs.statSync(abs).isDirectory() ? 'tree' : 'blob'
      return { href: `${repoUrl}/${kind}/${branch}/${encodeURI(rel)}${suffix}` }
    }

    const rewriteImage = (src: string, source: string): string | undefined => {
      if (ABSOLUTE_URL_RE.test(src) || src.startsWith('/') || src.startsWith('data:')) return undefined
      const resolved = resolve(src, source)
      if (!resolved) return undefined
      const { abs } = resolved
      if (source === page && inside(abs, srcDir)) return undefined
      if (!fs.existsSync(abs)) {
        problems.push(`image ${src} (in ${shown(source)}): ${shown(abs)} does not exist`)
        return undefined
      }
      if (devServer) return `${base}@fs${posix(abs).startsWith('/') ? '' : '/'}${posix(abs)}`
      // Relative to the page, so that Vite imports the file and copies it into the build.
      const rel = posix(path.relative(path.dirname(page), abs))
      return rel.startsWith('.') ? rel : './' + rel
    }

    let line: number | undefined
    for (const block of state.tokens) {
      if (block.map) line = block.map[0]
      if (block.type !== 'inline' || !block.children) continue
      const source = sourceAt(block.map?.[0] ?? line)
      for (const token of block.children) {
        if (token.type === 'link_open') {
          const href = token.attrGet('href')
          const to = href ? rewriteLink(href, source) : undefined
          if (to) {
            token.attrSet('href', to.href)
            if (to.target) token.attrSet('target', to.target)
          }
        } else if (token.type === 'image') {
          const src = token.attrGet('src')
          const to = src ? rewriteImage(src, source) : undefined
          if (to) token.attrSet('src', to)
        }
      }
    }

    if (problems.length) {
      const message = `[repo-links] ${posix(path.relative(srcDir, page))}: ${problems.length} broken link(s):\n  ${problems.join('\n  ')}`
      if (strict) throw new Error(message)
      console.warn(message)
    }
  })
}
