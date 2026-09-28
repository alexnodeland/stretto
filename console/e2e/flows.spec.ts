import { test, expect } from './fixtures'

test('the flows list shows how each flow decides, and a file that does not load', async ({
  page,
}) => {
  await page.goto('/flows')
  await expect(page.getByRole('link', { name: 'shop-promoted' })).toBeVisible()
  await expect(page.getByText('promoted 3/4')).toBeVisible()
  await expect(page.getByText('This file does not load')).toBeVisible()
})

test('the threshold previews which lookups act, on the graph and the sites', async ({ page }) => {
  await page.goto('/flows/retail')
  await expect(page.getByTestId('flow-graph')).toBeVisible()
  await expect(page.getByTestId('acting')).toContainText('after 4 of its 5 sites')
  const number = page.getByTestId('threshold-number')
  await number.fill('0.72')
  await number.press('Enter')
  await expect(page.getByTestId('acting')).toContainText('after 3 of its 5 sites')
  await expect(page.getByTestId('site-get_order_details')).toContainText(
    'hands back: 0.71 is below 0.72',
  )
  await expect(page).toHaveURL(/threshold=0\.72/)
  await page.getByRole('button', { name: 'Reset the threshold to 0.3' }).click()
  await expect(page.getByTestId('acting')).toContainText('after 4 of its 5 sites')
})

test('the flow page has its bindings, tools, review, raw JSON and a comparison', async ({
  page,
}) => {
  await page.goto('/flows/shop')
  await expect(page.getByTestId('binding-get_order_details')).toContainText('$.orders[*]')
  await expect(page.getByRole('cell', { name: 'cancel_pending_order' })).toBeVisible()
  await expect(page.getByText('never calls it')).toBeVisible()
  await page.getByRole('tab', { name: 'Review' }).click()
  await expect(page.getByRole('heading', { name: 'Flow: shop' })).toBeVisible()
  await page.getByRole('tab', { name: 'Raw JSON' }).click()
  await expect(page.getByText('stretto_flow')).toBeVisible()
  await page.getByRole('tab', { name: 'Compare' }).click()
  await page.getByTestId('compare-select').selectOption('retail')
  await expect(page.getByText('Nothing needs review')).toBeVisible()
  await page.locator('.cmp-swap').click()
  await expect(page.getByText(/Needs review: \d+/)).toBeVisible()
  await expect(page.getByTestId('diff-review')).toContainText(
    'find_user_id_by_name_zip is now marked read-only',
  )
})

test('promote and audit start from the flow', async ({ page }) => {
  await page.goto('/flows/shop')
  await page.getByRole('link', { name: 'Promote' }).click()
  await expect(page).toHaveURL(/\/jobs\/new\?kind=promote&flow=shop/)
  await expect(page.getByTestId('job-flow')).toHaveValue('shop')
  await expect(
    page.getByText('stretto promote --flow shop.flow.json --sessions shadow/shop'),
  ).toBeVisible()
})
