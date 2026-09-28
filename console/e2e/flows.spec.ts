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

test('a drift alarm shows on its flow, and a drift job starts from there', async ({ page }) => {
  await page.goto('/flows/shop-promoted')
  const alarm = page.getByTestId('drift-alarm')
  await expect(alarm).toContainText(
    'The agent may have changed under this flow: stretto drift sounded its alarm',
  )
  await expect(alarm).toContainText('a change likeliest 3 sessions ago, at 78%')
  await expect(alarm).toContainText('get_order_details, find_user_id_by_email')
  // A flow whose last drift run found nothing, or that never had one, shows none.
  await page.goto('/flows/shop')
  await expect(page.getByRole('heading', { name: 'shop', level: 1 })).toBeVisible()
  await expect(page.getByTestId('drift-alarm')).toHaveCount(0)

  await page.getByRole('link', { name: 'Drift' }).click()
  await expect(page).toHaveURL(/\/jobs\/new\?kind=drift&flow=shop/)
  await expect(page.getByTestId('job-flow')).toHaveValue('shop')
  await expect(
    page.getByText(
      'stretto drift --flow shop.flow.json --sessions logs/shop --window 10 --threshold 0.5',
    ),
  ).toBeVisible()
  await page.getByTestId('job-window').fill('2')
  await page.getByTestId('job-start').click()
  await expect(page.getByText('A whole number of sessions, 3 or more.')).toBeVisible()
  await page.getByTestId('job-window').fill('10')
  await page.getByTestId('job-start').click()
  // The shop's sessions include one that surprised its flow: the mock's alarm.
  await expect(page).toHaveURL(/\/jobs\/j-\d+$/)
  await expect(page.getByText('Alarm').first()).toBeVisible()
  await expect(page.getByRole('heading', { name: 'Drift of shop' })).toBeVisible()
  await page.goto('/flows/shop')
  await expect(page.getByTestId('drift-alarm')).toBeVisible()
})

test('a staged flow is compared with the committed one, committed and rolled back', async ({
  page,
}) => {
  await page.goto('/flows')
  await expect(page.getByTestId('staged-shop')).toHaveText('staged changes')
  await expect(page.getByTestId('staged-shop.staged')).toHaveText('staged')
  await page.goto('/flows/shop?tab=staged')
  await expect(page.getByTestId('stage-status')).toContainText('learned from 11 sessions')
  await expect(page.getByTestId('stage-total')).toContainText('12 lookups')
  await expect(page.getByTestId('stage-delta')).toHaveText('+1 used, −2 detours')
  await expect(page.getByText('What committing it would change')).toBeVisible()
  await expect(page.getByText('No version has been committed')).toBeVisible()

  // Committed, with a note: the flow as it was is kept first.
  await page.getByTestId('stage-commit').click()
  await expect(page.getByRole('dialog')).toContainText('as version 2')
  await page.getByTestId('stage-note').fill('reads the order after the account')
  await page.getByTestId('stage-commit-confirm').click()
  await expect(page.getByText('Committed as version 2')).toBeVisible()
  await expect(page.getByTestId('version-2')).toContainText('committed now')
  await expect(page.getByTestId('version-2')).toContainText('reads the order after the account')
  await expect(page.getByTestId('version-1')).toContainText('found in place')
  await expect(page.getByTestId('stage-same')).toBeVisible()
  await expect(page.getByTestId('stage-commit')).toBeDisabled()

  // Rolled back to the flow as it was, as a version of its own.
  await page.getByTestId('rollback-1').click()
  await page.getByTestId('rollback-note').fill('too few sessions yet')
  await page.getByTestId('rollback-confirm').click()
  await expect(page.getByText('Rolled back to version 1', { exact: true })).toBeVisible()
  await expect(page.getByTestId('version-3')).toContainText('rolled back to version 1')
  await expect(page.getByTestId('version-3')).toContainText('committed now')
  await expect(page.getByTestId('stage-commit')).toBeEnabled()
})

test('the staged flow’s page names its committed flow', async ({ page }) => {
  await page.goto('/flows/shop.staged')
  await expect(page.getByTestId('staged-of')).toContainText('The staged flow of shop')
  await page.getByTestId('staged-of').getByRole('link', { name: 'shop' }).click()
  await expect(page).toHaveURL(/\/flows\/shop$/)
})

test('a flow with nothing staged is staged by a job', async ({ page }) => {
  await page.goto('/flows/retail?tab=staged')
  await expect(page.getByTestId('stage-empty')).toContainText('Nothing is staged')
  await page.getByRole('link', { name: 'Stage this flow' }).click()
  await expect(page).toHaveURL(/\/jobs\/new\?kind=stage&flow=retail/)
  await expect(page.getByTestId('job-flow')).toHaveValue('retail')
  await expect(
    page.getByText(
      'stretto stage --flow flows/retail.flow.json --sessions logs/retail --window 50',
    ),
  ).toBeVisible()
  await page.getByTestId('job-start').click()
  await expect(page).toHaveURL(/\/jobs\/j-\d+/)
  await expect(page.getByTestId('job-artifacts')).toContainText('flows/retail.staged.flow.json')
  await page.goto('/flows/retail?tab=staged')
  await expect(page.getByTestId('stage-status')).toBeVisible()
  await expect(page.getByText('No session has been scored by both flows yet')).toBeVisible()
})
