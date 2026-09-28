import { test, expect, expectNoOverflow, sessionKey } from './fixtures'

test('the sidebar reaches every page', async ({ page }) => {
  await page.goto('/')
  const nav = page.getByRole('navigation', { name: 'Pages' })
  for (const [name, heading] of [
    ['Servers', 'Servers'],
    ['Sessions', 'Sessions'],
    ['Flows', 'Flows'],
    ['Jobs', 'Jobs'],
    ['Settings', 'Settings'],
    ['Overview', 'Overview'],
  ]) {
    await nav.getByRole('link', { name: name!, exact: true }).click()
    await expect(page.getByRole('heading', { name: heading!, level: 1 })).toBeVisible()
    await expect(nav.getByRole('link', { name: name!, exact: true })).toHaveAttribute(
      'aria-current',
      'page',
    )
  }
})

test('the command palette jumps to a flow, a server and a session', async ({ page }) => {
  await page.goto('/')
  await page.keyboard.press('Control+k')
  const input = page.getByTestId('palette-input')
  await expect(input).toBeFocused()
  await input.fill('shop-promoted')
  const results = page.getByRole('listbox', { name: 'Results' })
  await expect(results.getByRole('option', { name: /shop-promoted/ }).first()).toBeVisible()
  await page.keyboard.press('Enter')
  await expect(page).toHaveURL(/\/flows\/shop-promoted/)

  await page.getByTestId('palette-open').click()
  await input.fill('orders-api')
  await results.getByRole('option', { name: /orders-api/ }).click()
  await expect(page).toHaveURL(/\/servers\/orders-api/)

  const key = await sessionKey(page, 'shadow')
  await page.keyboard.press('Control+k')
  await input.fill(key.slice(0, 15))
  await expect(
    results.getByRole('option', { name: new RegExp(key.slice(0, 15)) }).first(),
  ).toBeVisible()
  await page.keyboard.press('ArrowDown')
  await page.keyboard.press('Escape')
  await expect(input).toBeHidden()
})

test('a wrong address says so', async ({ page }) => {
  await page.goto('/nowhere')
  await expect(page.getByText('Nothing is at this address')).toBeVisible()
})

test('on a phone, the menu opens as a drawer and no page scrolls sideways', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 })
  const served = await sessionKey(page, 'served')
  for (const path of [
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
  ]) {
    await page.goto(path)
    await expect(page.locator('h1').first()).toBeVisible()
    await page.waitForTimeout(150)
    await expectNoOverflow(page)
  }
  await page.goto('/')
  await page.getByRole('button', { name: 'Open the menu' }).click()
  const nav = page.getByRole('navigation', { name: 'Pages' })
  await nav.getByRole('link', { name: 'Flows' }).click()
  await expect(page.getByRole('heading', { name: 'Flows', level: 1 })).toBeVisible()
  await expect(page.getByRole('button', { name: 'Open the menu' })).toBeVisible()
})

test('the theme toggle cycles and is kept', async ({ page }) => {
  await page.goto('/settings')
  await page.getByRole('radio', { name: 'Dark' }).click()
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark')
  await page.reload()
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark')
  await page.getByRole('button', { name: /Theme: dark/ }).click()
  await expect(page.locator('html')).not.toHaveAttribute('data-theme', /./)
})
