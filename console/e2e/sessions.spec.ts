import { test, expect, sessionKey } from './fixtures'

test('the sessions list filters by mode and text, and keeps the filters in the address', async ({
  page,
}) => {
  await page.goto('/sessions')
  await expect(page.getByText('42 sessions')).toBeVisible()
  await page.getByRole('radio', { name: 'Shadow' }).click()
  await expect(page.getByText('3 sessions match')).toBeVisible()
  await expect(page).toHaveURL(/mode=shadow/)
  await page.getByRole('radio', { name: 'All' }).click()
  await page.getByTestId('sessions-search').fill('tickets')
  await expect(page.getByText('3 sessions match')).toBeVisible()
  await page.getByTestId('sessions-search').fill('nothing-like-this')
  await expect(page.getByText('No sessions match')).toBeVisible()
  await page.getByRole('button', { name: 'Clear the filters' }).click()
  await expect(page.getByText('42 sessions')).toBeVisible()
})

test('a served session shows each turn, and the lookups stretto read ahead under the call', async ({
  page,
}) => {
  const key = await sessionKey(page, 'served')
  await page.goto(`/sessions/${key}`)
  await expect(page.getByRole('heading', { name: key })).toBeVisible()
  await expect(page.getByText('Customer').first()).toBeVisible()
  await expect(page.getByText('Read ahead by stretto: 3 lookups').first()).toBeVisible()
  await expect(page.getByTestId('decision-made')).toHaveCount(3)
  await expect(page.getByText('get_order_details at 0.05, below 0.3').first()).toBeVisible()
  await expect(page.getByText('0.87').first()).toBeVisible()

  const firstCall = page.locator('.call').first()
  await firstCall.locator('.call-toggle').click()
  await expect(firstCall.getByText('Arguments', { exact: true })).toBeVisible()
  await expect(firstCall.getByText('"c', { exact: false }).first()).toBeVisible()
  await firstCall.locator('.dr-toggle').first().click()
  await expect(
    firstCall.getByText('"user_id"').or(firstCall.getByText('user_id')).first(),
  ).toBeVisible()
})

test('a session that surprised its flow is marked in the list, and says where on its page', async ({
  page,
}) => {
  await page.goto('/sessions?mode=served')
  const badge = page.getByTestId('session-surprised')
  await expect(badge).toHaveCount(1)
  await page.locator('tr', { has: badge }).locator('a.st-id').click()
  const notice = page.getByTestId('surprised-notice')
  await expect(notice).toContainText('The session surprised the flow after call')
  await expect(notice).toContainText(
    "5 of the agent's steps in a row averaged 3.42 nats, above 3.10",
  )
  await expect(notice).toContainText('From there the flow handed back after every call.')
})

test('the decisions tab lists every decision, and the raw log its lines', async ({ page }) => {
  const key = await sessionKey(page, 'served')
  await page.goto(`/sessions/${key}`)
  await page.getByRole('tab', { name: /Decisions/ }).click()
  await expect(page).toHaveURL(/tab=decisions/)
  await expect(page.locator('table.dt tr.decision').first()).toContainText('looks up')
  await page.getByRole('tab', { name: /Raw log/ }).click()
  await expect(page.getByText('tools/call find_user_id_by_email').first()).toBeVisible()
  await page.getByRole('radio', { name: 'JSON lines' }).click()
  await expect(page.locator('ol.lines li').first()).toContainText('stretto_mcp_log')
  await page.getByRole('tab', { name: /Tools/ }).click()
  await expect(page.getByRole('cell', { name: 'cancel_pending_order' })).toBeVisible()
})

test('a shadow session shows what the flow would have looked up', async ({ page }) => {
  const key = await sessionKey(page, 'shadow')
  await page.goto(`/sessions/${key}`)
  await expect(
    page.getByText('In shadow: what the flow would have read ahead').first(),
  ).toBeVisible()
  await expect(page.getByTestId('decision-shadow').first()).toContainText('Would look up')
})

test('deleting a session moves it to the trash', async ({ page }) => {
  const key = await sessionKey(page, 'recorded', 'notes')
  await page.goto(`/sessions/${key}`)
  await page.getByRole('button', { name: 'Delete' }).click()
  await page.getByRole('button', { name: 'Move to the trash' }).click()
  await expect(page.getByText('Session moved to the trash')).toBeVisible()
  await expect(page).toHaveURL(/\/sessions$/)
  await expect(page.getByText('41 sessions')).toBeVisible()
})
