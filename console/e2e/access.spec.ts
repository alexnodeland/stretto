import { test, expect, mockState } from './fixtures'

test('without the token, the console asks for it; the right one signs in', async ({ page }) => {
  const { token } = await mockState(page, { auth: true })
  await page.goto('/flows')
  await expect(page.getByTestId('sign-in')).toBeVisible()
  await page.getByLabel('Token', { exact: true }).fill('not-it')
  await page.getByRole('button', { name: 'Sign in' }).click()
  await expect(page.getByRole('alert')).toContainText('not accepted')
  await page.getByLabel('Token', { exact: true }).fill(String(token))
  await page.getByRole('button', { name: 'Sign in' }).click()
  await expect(page.getByRole('heading', { name: 'Flows', level: 1 })).toBeVisible()
  await expect(page).toHaveURL(/\/flows$/)
  const cookies = await page.context().cookies()
  expect(cookies.find((c) => c.name === 'stretto_console')?.httpOnly).toBe(true)
})

test('signing out asks for the token again', async ({ page }) => {
  const { token } = await mockState(page, { auth: true })
  await page.goto(`/settings?token=${token}`)
  await expect(page.getByRole('heading', { name: 'Settings', level: 1 })).toBeVisible()
  await page.getByRole('button', { name: 'Sign out' }).click()
  await expect(page.getByTestId('sign-in')).toBeVisible()
})

test('read-only: the badge shows, and writes are off', async ({ page }) => {
  await mockState(page, { read_only: true })
  await page.goto('/servers')
  await expect(page.getByTestId('read-only')).toBeVisible()
  await expect(page.getByRole('button', { name: 'Add a server' })).toBeDisabled()
  await page.goto('/servers/shop')
  await expect(page.getByTestId('probe')).toBeDisabled()
  const response = await page.request.post('/api/jobs', {
    data: { kind: 'doctor' },
    headers: { 'X-Stretto-Console': '1' },
  })
  expect(response.status()).toBe(403)
})

test('the API refuses a write without the console’s header', async ({ page }) => {
  const response = await page.request.post('/api/jobs', { data: { kind: 'doctor' } })
  expect(response.status()).toBe(403)
  expect((await response.json()).error).toMatch(/X-Stretto-Console/)
})
