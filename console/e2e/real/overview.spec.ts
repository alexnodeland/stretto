import { test, expect, expectNoOverflow } from './fixtures'

test('the overview counts what the data directory holds, domain by domain', async ({ page }) => {
  await page.goto('/')
  await expect(page.getByRole('heading', { name: 'Overview', level: 1 })).toBeVisible()
  const totals = page.getByRole('region', { name: 'Totals' })
  await expect(totals.getByText('Sessions this week')).toBeVisible()
  await expect(totals.getByText('12', { exact: true })).toBeVisible()
  await expect(totals.getByText('Lookups served')).toBeVisible()
  await expect(
    page.getByRole('img', { name: /Tool calls per day over the last 14 days/ }),
  ).toBeVisible()
  for (const domain of ['shop', 'retail', 'orders']) {
    await expect(page.getByRole('link', { name: domain, exact: true }).first()).toBeVisible()
  }
  await expect(page.getByText(/exists and is writable/)).toBeVisible()
  await page.getByRole('button', { name: 'Table' }).click()
  await expect(page.locator('figure table tbody tr')).toHaveCount(14)
})

test('the overview reads on a phone', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 })
  await page.goto('/')
  await expect(page.getByRole('heading', { name: 'Overview', level: 1 })).toBeVisible()
  await expect(page.getByText('Recent sessions')).toBeVisible()
  await expectNoOverflow(page)
})

test('a page that is not there says so', async ({ page }) => {
  await page.goto('/nowhere')
  await expect(page.getByRole('heading', { name: 'Page not found' })).toBeAttached()
  await expect(page.getByRole('link', { name: 'Overview' }).first()).toBeVisible()
})
