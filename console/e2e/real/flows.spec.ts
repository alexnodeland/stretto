import { test, expect } from './fixtures'

test('the flows list shows how each flow decides and where it was promoted', async ({ page }) => {
  await page.goto('/flows')
  await expect(page.getByRole('link', { name: 'shop', exact: true }).first()).toBeVisible()
  await expect(page.getByRole('link', { name: 'shop.promoted' })).toBeVisible()
  await expect(page.getByText('promoted 3/4')).toBeVisible()
})

test('a flow’s graph and sites follow the threshold', async ({ page }) => {
  await page.goto('/flows/shop')
  await expect(page.getByTestId('flow-graph')).toBeVisible()
  await expect(page.getByTestId('flow-graph').locator('.vue-flow__node')).toHaveCount(3)
  await expect(page.getByTestId('acting')).toContainText('after 3 of its 3 sites')
  const number = page.getByTestId('threshold-number')
  await number.fill('0.5')
  await number.press('Enter')
  await expect(page.getByTestId('acting')).toContainText('after 2 of its 3 sites')
  await expect(page.getByTestId('site-get_order_details')).toContainText(
    'hands back: 0.46 is below 0.50',
  )
  await expect(page).toHaveURL(/threshold=0\.5/)
  await page.getByRole('button', { name: 'Reset the threshold to 0.3' }).click()
  await expect(page.getByTestId('acting')).toContainText('after 3 of its 3 sites')
})

test('a flow has its bindings, tools, review, raw JSON and a comparison', async ({ page }) => {
  await page.goto('/flows/shop')
  await expect(page.getByTestId('binding-get_order_details')).toContainText('$.orders[*]')
  await expect(page.getByRole('cell', { name: 'cancel_pending_order' })).toBeVisible()
  await expect(page.getByText('never calls it')).toBeVisible()
  await page.getByRole('tab', { name: 'Review' }).click()
  await expect(page.getByRole('heading', { name: 'Flow: shop' })).toBeVisible()
  await page.getByRole('tab', { name: 'Raw JSON' }).click()
  await expect(page.getByText('stretto_flow')).toBeVisible()
  await page.getByRole('tab', { name: 'Compare' }).click()
  await page.getByTestId('compare-select').selectOption('shop.promoted')
  await expect(page.getByText('Nothing needs review')).toBeVisible()
  await expect(page.getByRole('heading', { name: 'Promotion', level: 4 })).toBeVisible()
  await expect(page.getByText(/3 of 4 sites scored/)).toBeVisible()
})

test('a promoted flow shows where it may act, and the promotion’s bar', async ({ page }) => {
  await page.goto('/flows')
  await page.getByRole('link', { name: 'shop.promoted' }).click()
  await expect(page.getByRole('heading', { name: 'shop.promoted' })).toBeVisible()
  await expect(page.getByTestId('acting')).toContainText('where promoted')
  await expect(page.getByText('promoted 3/4').first()).toBeVisible()
})

test('promote and audit start from the flow', async ({ page }) => {
  await page.goto('/flows/shop')
  await page.getByRole('link', { name: 'Promote' }).click()
  await expect(page).toHaveURL(/\/jobs\/new\?kind=promote&flow=shop/)
  await expect(page.getByTestId('job-flow')).toHaveValue('shop')
})
