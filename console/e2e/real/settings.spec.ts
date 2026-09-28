import { test, expect } from './fixtures'

test('settings show the data dir, the installation, access and the theme', async ({ page }) => {
  await page.goto('/settings')
  await expect(page.getByText(/test-results\/real\/home/).first()).toBeVisible()
  await expect(page.getByText('Session logs').first()).toBeVisible()
  await expect(page.getByText(/\/stretto-proxy$/).first()).toBeVisible()
  await expect(page.getByText('--no-auth')).toBeVisible()
  await page.getByRole('radio', { name: 'Dark' }).click()
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark')
  await page.getByRole('radio', { name: 'System' }).click()
})

test('the palette jumps to a page, a server or a flow', async ({ page }) => {
  await page.goto('/')
  await page.keyboard.press('Control+k')
  await page.getByTestId('palette-input').fill('shop.promoted')
  await page.keyboard.press('Enter')
  await expect(page).toHaveURL(/\/flows\/shop\.promoted$/)
})
