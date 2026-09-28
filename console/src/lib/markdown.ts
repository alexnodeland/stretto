/**
 * Markdown from the server (a flow's review, an audit's report) as HTML.
 * The text holds tool names and descriptions a server supplied, so raw HTML
 * is escaped (`html: false`) and links are markdown-it's validated ones,
 * opened in a new tab without a referrer.
 */
import MarkdownIt from 'markdown-it'

const md = new MarkdownIt({ html: false, linkify: false, typographer: false })

md.renderer.rules.link_open = (tokens, idx, options, _env, self) => {
  const token = tokens[idx]!
  token.attrSet('target', '_blank')
  token.attrSet('rel', 'noopener noreferrer')
  return self.renderToken(tokens, idx, options)
}
md.renderer.rules.table_open = () => '<div class="md-table"><table>\n'
md.renderer.rules.table_close = () => '</table></div>\n'

export function renderMarkdown(text: string): string {
  return md.render(text)
}
