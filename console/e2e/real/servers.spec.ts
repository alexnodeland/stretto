import { test, expect } from './fixtures'

test('the servers page lists the registry and the upstreams seen in sessions', async ({ page }) => {
  await page.goto('/servers')
  await expect(page.getByTestId('server-shop')).toContainText('Serving')
  await expect(page.getByTestId('server-orders')).toContainText(
    'https://mcp.example.com/orders/mcp',
  )
  await expect(page.getByTestId('discovered-shop')).toContainText('registered')
  await expect(
    page.getByTestId('discovered-retail').getByRole('link', { name: 'Add' }),
  ).toBeVisible()
})

test('a server gives each host’s configuration, and its connection test lists the tools', async ({
  page,
}) => {
  await page.goto('/servers')
  await page.getByTestId('server-shop').getByRole('link', { name: 'shop' }).first().click()
  await expect(page.getByRole('heading', { name: 'shop', level: 1 })).toBeVisible()
  const snippet = page.getByTestId('host-snippet')
  await expect(snippet).toContainText('claude mcp add shop -- stretto-proxy --record')
  await page.getByRole('tab', { name: 'Cursor' }).click()
  await expect(snippet).toContainText('"mcpServers"')
  await page.getByTestId('probe').click()
  await expect(page.getByTestId('probe-ok')).toBeVisible({ timeout: 20_000 })
  await expect(page.getByRole('cell', { name: 'cancel_pending_order' })).toBeVisible()
})

test('a discovered upstream becomes a server, which can be edited and removed', async ({
  page,
}) => {
  await page.goto('/servers')
  await page.getByTestId('discovered-retail').getByRole('link', { name: 'Add' }).click()
  await expect(page.getByTestId('server-name')).toHaveValue('retail')
  await expect(page.getByTestId('server-command')).toHaveValue('stretto-mcp-demo --world retail')
  await page.getByTestId('server-save').click()
  await expect(page).toHaveURL(/\/servers\/retail$/)
  await expect(page.getByText('Server retail added')).toBeVisible()
  await expect(page.getByTestId('host-snippet')).toContainText('claude mcp add retail')

  await page.getByRole('link', { name: 'Edit' }).click()
  await expect(page.getByTestId('server-name')).toBeDisabled()
  await page.getByPlaceholder('What it serves').fill('The retail world, recorded')
  await page.getByTestId('server-save').click()
  await expect(page.getByText('Server retail saved')).toBeVisible()
  await expect(page.getByText('The retail world, recorded')).toBeVisible()

  await page.getByRole('button', { name: 'Remove' }).click()
  await page
    .getByRole('dialog')
    .getByRole('button', { name: /Remove/ })
    .click()
  await expect(page).toHaveURL(/\/servers$/)
  await expect(page.getByTestId('server-retail')).toHaveCount(0)
})

test('the server refuses a duplicate name', async ({ page }) => {
  await page.goto('/servers/new?domain=shop')
  await page.getByTestId('server-command').fill('stretto-mcp-demo')
  await page.getByTestId('server-save').click()
  await expect(page.getByText('A server named shop exists.')).toBeVisible()
})
