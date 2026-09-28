import { test as base, expect, type Page } from '@playwright/test'

/** The mock's state, reset before each test so they do not see each other's servers and jobs. */
export async function mockState(page: Page, state: Record<string, unknown>) {
  const response = await page.request.post('/__mock/state', { data: state })
  expect(response.ok()).toBeTruthy()
  return response.json() as Promise<Record<string, unknown>>
}

export const test = base.extend<{ reset: void }>({
  reset: [
    async ({ page }, use) => {
      await mockState(page, { reset: true, latency: 20 })
      await use()
    },
    { auto: true },
  ],
})

export { expect }

/** The first session of a mode, from the mock's API. */
export async function sessionKey(
  page: Page,
  mode: 'served' | 'shadow' | 'recorded',
  q = '',
): Promise<string> {
  const response = await page.request.get(`/api/sessions?mode=${mode}&limit=1${q ? `&q=${q}` : ''}`)
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
