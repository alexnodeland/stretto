import { test, expect } from './fixtures'

test('adding a server checks the form, then gives each host’s configuration', async ({
  page,
  context,
}) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write'])
  await page.goto('/servers')
  await page.getByRole('link', { name: 'Add a server' }).first().click()
  await expect(page.getByRole('heading', { name: 'Add a server', level: 1 })).toBeVisible()
  await page.getByTestId('server-save').click()
  await expect(page.getByText('Give the server a name.')).toBeVisible()
  await expect(page.getByText('Give the command that starts the server.')).toBeVisible()

  await page.getByTestId('server-name').fill('Demo Shop')
  await page.getByTestId('server-save').click()
  await expect(page.getByText('Use lowercase letters, digits, - and _.')).toBeVisible()
  await page.getByTestId('server-name').fill('demo')
  await page.getByTestId('server-command').fill('stretto-mcp-demo --world retail')
  await expect(page.locator('.words .chip')).toHaveText(['stretto-mcp-demo', '--world', 'retail'])
  await expect(
    page.getByText(
      'stretto-proxy --record ~/.stretto/logs/demo --domain demo -- stretto-mcp-demo --world retail',
    ),
  ).toBeVisible()
  await page.getByTestId('server-save').click()

  await expect(page).toHaveURL(/\/servers\/demo$/)
  await expect(page.getByText('Server demo added')).toBeVisible()
  const snippet = page.getByTestId('host-snippet')
  await expect(snippet).toContainText(
    'claude mcp add demo -- stretto-proxy --record ~/.stretto/logs/demo',
  )
  await page.getByRole('tab', { name: 'Cursor' }).click()
  await expect(snippet).toContainText('"mcpServers"')
  await page.getByRole('tab', { name: 'VS Code' }).click()
  await expect(snippet).toContainText('"type": "stdio"')
  await snippet.getByRole('button', { name: 'Copy' }).click()
  await expect(page.getByText('Configuration copied')).toBeVisible()
  const copied = await page.evaluate(() => navigator.clipboard.readText())
  expect(copied).toContain('"servers"')
})

test('a retail server takes the guards, the judge, stretto_commit and retention', async ({
  page,
}) => {
  await page.goto('/servers/new')
  await page.getByTestId('server-name').fill('retail')
  await page.getByTestId('server-command').fill('retail-mcp')
  await page.getByTestId('server-guards').check()
  await page.getByTestId('server-judge').selectOption('log')
  await page.getByTestId('server-context').fill('~/.stretto/context/retail.jsonl')
  await page.getByTestId('server-commit').check()
  await page.getByTestId('server-retain').fill('30')
  await expect(
    page.getByText(
      'stretto-proxy --record ~/.stretto/logs/retail --domain retail --retain-days 30 --guards --confirm-judge log --context ~/.stretto/context/retail.jsonl --commit -- retail-mcp',
    ),
  ).toBeVisible()
  await page.getByTestId('server-save').click()

  await expect(page).toHaveURL(/\/servers\/retail$/)
  await expect(page.getByTestId('server-guards')).toContainText(
    'on, with the confirmation judge (log) reading ~/.stretto/context/retail.jsonl',
  )
  await expect(page.getByTestId('server-retain')).toHaveText('30 days')
  await expect(page.getByTestId('host-snippet')).toContainText(
    '--retain-days 30 --guards --confirm-judge log',
  )
})

test('a served server sets its flow’s surprise gate, and the proxy gets it', async ({ page }) => {
  await page.goto('/servers/shop')
  await expect(page.getByTestId('server-surprise')).toHaveText('3.1 nats, in place of the flow’s')
  await expect(page.getByTestId('host-snippet')).toContainText('--flow-surprise 3.1')
  await page.goto('/servers/shop/edit')
  await expect(page.getByTestId('server-surprise')).toHaveValue('threshold')
  await expect(page.getByTestId('server-surprise-nats')).toHaveValue('3.1')
  await page.getByTestId('server-surprise').selectOption('off')
  await expect(page.getByText('--flow-surprise off')).toBeVisible()
  await page.getByTestId('server-save').click()
  await expect(page).toHaveURL(/\/servers\/shop$/)
  await expect(page.getByTestId('server-surprise')).toHaveText('off')
  await expect(page.getByTestId('host-snippet')).toContainText('--flow-surprise off')
})

test('a served server keeps its flow to some of its lookups', async ({ page }) => {
  await page.goto('/servers/shop/edit')
  await page.getByTestId('server-flow-tools-only').check()
  await page.getByTestId('server-flow-tool-get_user_details').check()
  await expect(page.getByText('--flow-tools get_user_details')).toBeVisible()
  await page.getByTestId('server-save').click()
  await expect(page).toHaveURL(/\/servers\/shop$/)
  await expect(page.getByTestId('server-flow-tools')).toHaveText('only get_user_details')
  await expect(page.getByTestId('host-snippet')).toContainText('--flow-tools get_user_details')
})

test('testing the connection lists the tools and their kinds', async ({ page }) => {
  await page.goto('/servers/shop')
  await page.getByTestId('probe').click()
  await expect(page.getByTestId('probe-ok')).toContainText('stretto-mcp-demo 0.1.0')
  await expect(page.getByRole('cell', { name: 'cancel_pending_order' })).toBeVisible()
  await expect(page.locator('.probe-tools tbody tr')).toHaveCount(4)
  await expect(page.locator('.probe-tools').getByText('write')).toBeVisible()
})

test('a server that cannot be reached says why', async ({ page }) => {
  await page.goto('/servers/orders-api')
  await expect(page.getByText('flow not found: ~/.stretto/orders.flow.json')).toBeVisible()
  await page.getByTestId('probe').click()
  await expect(page.getByTestId('probe-failed')).toContainText('ORDERS_AUTH, which is not set')
})

test('a discovered upstream can be added with its URL filled in', async ({ page }) => {
  await page.goto('/servers')
  await page.getByTestId('discovered-tickets').getByRole('link', { name: 'Add' }).click()
  await expect(page.getByTestId('server-name')).toHaveValue('tickets')
  await expect(page.getByTestId('server-url')).toHaveValue('https://desk.example.com/mcp')
})

test('editing keeps the name, and the server refuses a duplicate', async ({ page }) => {
  await page.goto('/servers/notes/edit')
  await expect(page.getByTestId('server-name')).toBeDisabled()
  await page.getByRole('radio', { name: /Shadow/ }).check({ force: true })
  await page.getByTestId('server-save').click()
  await expect(page.getByText('Shadow mode runs a flow: choose one.')).toBeVisible()
  await page.getByRole('radio', { name: /Record/ }).check({ force: true })
  await page.getByTestId('server-save').click()
  await expect(page.getByText('Server notes saved')).toBeVisible()

  await page.goto('/servers/new?domain=shop')
  await page.getByTestId('server-command').fill('stretto-mcp-demo')
  await page.getByTestId('server-save').click()
  await expect(page.getByText('A server named shop exists.')).toBeVisible()
})
