import { test, expect, expectNoOverflow } from './fixtures'

test('the sessions list filters by mode and text', async ({ page }) => {
  await page.goto('/sessions')
  await expect(page.getByText('12 sessions')).toBeVisible()
  await page.getByRole('radio', { name: 'Served' }).click()
  await expect(page.getByText('2 sessions match')).toBeVisible()
  await page.getByRole('radio', { name: 'Shadow' }).click()
  await expect(page.getByText('3 sessions match')).toBeVisible()
  await page.getByRole('radio', { name: 'All' }).click()
  await page.getByTestId('sessions-search').fill('retail')
  await expect(page.getByText('1 session matches')).toBeVisible()
})

test('a served session shows the lookups stretto read ahead, its decisions, log and tools', async ({
  page,
}) => {
  await page.goto('/sessions?mode=served')
  await page.getByRole('link', { name: '20260928T020401.195Z-14715' }).first().click()
  await expect(page.getByRole('heading', { name: '20260928T020401.195Z-14715' })).toBeVisible()
  await expect(page.getByText('Customer').first()).toBeVisible()
  await expect(page.getByText('Read ahead by stretto: 3 lookups')).toBeVisible()
  await expect(page.getByTestId('decision-made')).toHaveCount(3)
  await expect(page.getByText('get_order_details at 0.05, below 0.3').first()).toBeVisible()

  await page.getByRole('tab', { name: /Decisions/ }).click()
  await expect(page.locator('table.dt tr.decision')).toHaveCount(4)
  await expect(page.locator('table.dt tr.decision').first()).toContainText('looks up')
  await page.getByRole('tab', { name: /Raw log/ }).click()
  await expect(page.getByText('find_user_id_by_email {"email":"c42@example.com"}')).toBeVisible()
  await page.getByRole('radio', { name: 'JSON lines' }).click()
  await expect(page.locator('ol.lines li').first()).toContainText('stretto_mcp_log')
  await page.getByRole('tab', { name: /Tools/ }).click()
  await expect(page.getByRole('cell', { name: 'cancel_pending_order' })).toBeVisible()
  await expect(page.getByText('order_id:string!, reason:string!')).toBeVisible()
})

test('a shadow session shows what the flow would have looked up', async ({ page }) => {
  await page.goto('/sessions?mode=shadow')
  await page.locator('.st-table tbody tr').first().getByRole('link').first().click()
  await expect(
    page.getByText('In shadow: what the flow would have read ahead').first(),
  ).toBeVisible()
  await expect(page.getByTestId('decision-shadow').first()).toContainText('Would look up')
})

test('a session reads on a phone', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 })
  await page.goto('/sessions?mode=served')
  await page
    .getByRole('link', { name: /20260928T020401\.195Z-14715/ })
    .first()
    .click()
  await expect(page.getByText('Read ahead by stretto: 3 lookups')).toBeVisible()
  await expectNoOverflow(page)
})

test('deleting a session moves it to the trash', async ({ page }) => {
  await page.goto('/sessions?q=retail')
  await page.getByRole('link', { name: '20260928T020401.312Z-14780' }).first().click()
  await expect(page.getByRole('heading', { name: '20260928T020401.312Z-14780' })).toBeVisible()
  await page.getByRole('button', { name: 'Delete' }).click()
  await page.getByRole('button', { name: 'Move to the trash' }).click()
  await expect(page.getByText('Session moved to the trash')).toBeVisible()
  await expect(page).toHaveURL(/\/sessions$/)
  await expect(page.getByText('11 sessions')).toBeVisible()
})
