import { test, expect, mockState } from './fixtures'

test('the overview shows the totals, the last 14 days, each domain and health', async ({
  page,
}) => {
  await page.goto('/')
  await expect(page.getByRole('heading', { name: 'Overview', level: 1 })).toBeVisible()
  const totals = page.getByRole('region', { name: 'Totals' })
  await expect(totals.getByText('Sessions this week')).toBeVisible()
  await expect(totals.getByText('Lookups served')).toBeVisible()
  await expect(totals.getByText('read ahead by flows, every session')).toBeVisible()
  await expect(
    page.getByRole('img', { name: /Tool calls per day over the last 14 days/ }),
  ).toBeVisible()
  for (const domain of ['shop', 'notes', 'tickets']) {
    await expect(page.getByRole('link', { name: domain, exact: true }).first()).toBeVisible()
  }
  await expect(page.getByText('scratch/orders.flow.json does not load')).toBeVisible()
  await expect(page.getByText('Server orders-api: flow not found')).toBeVisible()
  await expect(page.getByText('Get started')).toHaveCount(0)
})

test('the chart has a table with every day', async ({ page }) => {
  await page.goto('/')
  await page.getByRole('button', { name: 'Table' }).click()
  await expect(page.locator('figure table tbody tr')).toHaveCount(14)
})

test('the domain filter narrows the overview’s cards and lists', async ({ page }) => {
  await page.goto('/')
  await page.getByTestId('domain-filter').selectOption('notes')
  await expect(page.getByText('The totals and the chart cover every domain')).toBeVisible()
  await expect(page.locator('article.dc')).toHaveCount(1)
  await page.getByTestId('domain-filter').selectOption({ label: 'All domains' })
  await expect(page.locator('article.dc')).toHaveCount(3)
})

test('an empty data dir says what to do next', async ({ page }) => {
  await mockState(page, { empty: true })
  await page.goto('/')
  await expect(page.getByRole('heading', { name: 'Get started' })).toBeVisible()
  await expect(page.getByText('Add a server', { exact: true }).first()).toBeVisible()
  await expect(page.getByText('Use your agent through it', { exact: true })).toBeVisible()
  await expect(page.getByText('Learn a flow').first()).toBeVisible()
  await expect(page.getByText('No sessions yet')).toBeVisible()
  await page
    .getByRole('navigation', { name: 'Pages' })
    .getByRole('link', { name: 'Sessions', exact: true })
    .click()
  await expect(page.getByText('No sessions recorded yet')).toBeVisible()
  await expect(
    page.getByText('Add a server, use your agent through it, then learn a flow.'),
  ).toBeVisible()
})
