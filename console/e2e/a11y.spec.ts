import AxeBuilder from '@axe-core/playwright'
import type { Page } from '@playwright/test'
import { test, expect, mockState, sessionKey } from './fixtures'

/** axe-core's WCAG 2.1 A and AA violations on the page as it is, one line each. */
async function violations(page: Page, where: string): Promise<string[]> {
  const { violations } = await new AxeBuilder({ page })
    .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
    .analyze()
  return violations.map(
    (v) =>
      `${where}: ${v.id} (${v.impact}): ${v.help}\n    ${v.nodes
        .slice(0, 4)
        .map(
          (n) =>
            `${n.target.join(' ')} — ${n.failureSummary?.split('\n').slice(1, 2).join(' ').trim()}`,
        )
        .join('\n    ')}${v.nodes.length > 4 ? `\n    …and ${v.nodes.length - 4} more` : ''}`,
  )
}

/**
 * Every page, in each theme, against axe-core's WCAG 2.1 A and AA rules:
 * names and roles, labels, landmarks, and the contrast of text on its
 * background.
 */
const pages = (served: string) => [
  '/',
  '/servers',
  '/servers/shop',
  '/servers/new',
  '/sessions',
  `/sessions/${served}`,
  '/flows',
  '/flows/retail',
  '/jobs',
  '/jobs/new',
  '/jobs/j-0003',
  '/settings',
]

for (const scheme of ['light', 'dark'] as const) {
  test(`every page passes axe's WCAG 2.1 A and AA rules, ${scheme}`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme })
    const served = await sessionKey(page, 'served')
    const found: string[] = []
    for (const path of pages(served)) {
      await page.goto(path)
      await expect(page.locator('h1').first()).toBeVisible()
      await page.waitForTimeout(400)
      found.push(...(await violations(page, path)))
    }
    expect(found, found.join('\n')).toEqual([])
  })
}

test('dialogs, the palette, sign-in, first run and a phone pass the same rules', async ({
  page,
}) => {
  const found: string[] = []
  // The command palette.
  await page.goto('/')
  await expect(page.locator('h1').first()).toBeVisible()
  await page.keyboard.press('Control+k')
  await expect(page.getByRole('dialog')).toBeVisible()
  found.push(...(await violations(page, 'the palette')))
  await page.keyboard.press('Escape')

  // A confirmation: deleting a session.
  await page.goto(`/sessions/${await sessionKey(page, 'recorded')}`)
  await page.getByRole('button', { name: 'Delete' }).click()
  await expect(page.getByRole('dialog')).toBeVisible()
  found.push(...(await violations(page, 'delete a session')))
  await page.keyboard.press('Escape')

  // A phone, with the menu open.
  await page.setViewportSize({ width: 390, height: 844 })
  await page.goto('/')
  await expect(page.locator('h1').first()).toBeVisible()
  found.push(...(await violations(page, 'the overview on a phone')))
  await page.getByRole('button', { name: 'Open the menu' }).click()
  await expect(page.getByRole('navigation', { name: 'Pages' })).toBeVisible()
  found.push(...(await violations(page, 'the menu on a phone')))
  await page.setViewportSize({ width: 1280, height: 800 })

  // First run: no data yet.
  await mockState(page, { empty: true })
  await page.goto('/')
  await expect(page.getByRole('heading', { name: 'Get started' })).toBeVisible()
  found.push(...(await violations(page, 'first run')))

  // Sign-in, and read-only.
  await mockState(page, { reset: true, auth: true })
  await page.goto('/')
  await expect(page.getByTestId('sign-in')).toBeVisible()
  found.push(...(await violations(page, 'sign-in')))
  await mockState(page, { reset: true, read_only: true })
  await page.goto('/servers')
  await expect(page.getByTestId('read-only')).toBeVisible()
  found.push(...(await violations(page, 'read-only')))

  expect(found, found.join('\n')).toEqual([])
})
