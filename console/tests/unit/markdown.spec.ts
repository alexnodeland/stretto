import { describe, expect, it } from 'vitest'
import { renderMarkdown } from '@/lib/markdown'

describe('markdown from the server', () => {
  it('renders a flow review: headings, code, and tables in a scroll box', () => {
    const html = renderMarkdown(
      '# Flow: shop\n\n| After | Lookups |\n|---|---|\n| `find_user_id_by_email` | `get_user_details` (6) |\n',
    )
    expect(html).toContain('<h1>Flow: shop</h1>')
    expect(html).toContain(
      '<div class="md-table" tabindex="0" role="region" aria-label="Table"><table>',
    )
    expect(html).toContain('<code>find_user_id_by_email</code>')
  })

  it('escapes raw HTML a tool description could carry', () => {
    const html = renderMarkdown(
      '- `evil`: <img src=x onerror=alert(1)> and <script>alert(2)</script>',
    )
    expect(html).not.toContain('<img')
    expect(html).not.toContain('<script')
    expect(html).toContain('&lt;script&gt;')
  })

  it('opens links in a new tab without a referrer, and drops javascript: links', () => {
    const html = renderMarkdown(
      '[privacy](https://github.com/alexnodeland/stretto/blob/main/docs/privacy.md) [x](javascript:alert(1))',
    )
    expect(html).toContain('target="_blank"')
    expect(html).toContain('rel="noopener noreferrer"')
    expect(html).not.toContain('href="javascript:')
  })
})
