import { test as base, expect, type Page } from '@playwright/test'

/**
 * The tests against the real console share its data directory, a copy of
 * crates/stretto-console/tests/fixtures/home: six sessions of the shop
 * recorded by the quickstart, two served with its flow, three in shadow, one
 * of the retail domain with a failed call, the shop's flow and its promotion,
 * and two servers (shop over stdio, orders over Streamable HTTP).
 *
 * Every page is checked for errors in the browser's console, a violation of
 * the server's Content-Security-Policy among them.
 */
export const test = base.extend<{ quiet: void }>({
  quiet: [
    async ({ page }, use) => {
      const problems: string[] = []
      page.on('pageerror', (e) => problems.push(`page error: ${e.message}`))
      page.on('console', (m) => {
        // A refused request is logged too; the tests check what the page says about it.
        if (m.type() === 'error' && !m.text().startsWith('Failed to load resource'))
          problems.push(`console: ${m.text()}`)
      })
      await use()
      expect(problems, 'errors in the page').toEqual([])
    },
    { auto: true },
  ],
})

export { expect }

/** A session's key, from the API. */
export async function sessionKey(page: Page, query: string): Promise<string> {
  const response = await page.request.get(`/api/sessions?${query}&limit=1`)
  expect(response.ok()).toBeTruthy()
  const body = (await response.json()) as { items: { key: string }[] }
  return body.items[0]!.key
}

/** No horizontal page scroll. */
export async function expectNoOverflow(page: Page) {
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - window.innerWidth,
  )
  expect(overflow, 'horizontal overflow in px').toBeLessThanOrEqual(0)
}
