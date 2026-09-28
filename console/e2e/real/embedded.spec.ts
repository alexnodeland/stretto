import { test, expect } from './fixtures'

const API = `http://127.0.0.1:${process.env.E2E_API_PORT ?? 7891}`

test('the console binary serves the built UI itself, under its policy', async ({
  page,
  request,
}) => {
  const response = await request.get(`${API}/`)
  expect(response.ok()).toBeTruthy()
  expect(response.headers()['content-security-policy']).toContain("default-src 'self'")
  const index = await response.text()
  // A debug build reads console/dist from the checkout that built it.
  test.skip(
    !index.includes('/theme-init.js'),
    'this stretto-console was built from another checkout, whose console/dist it serves',
  )
  await page.goto(`${API}/`)
  await expect(page.getByRole('heading', { name: 'Overview', level: 1 })).toBeVisible()
  await expect(page.getByText('Recent sessions')).toBeVisible()
})

test('the binary answers a session’s and a flow’s address with the UI, dots and all', async ({
  page,
  request,
}) => {
  const index = await (await request.get(`${API}/`)).text()
  test.skip(
    !index.includes('/theme-init.js'),
    'this stretto-console was built from another checkout, whose console/dist it serves',
  )
  // Loaded by address, as a reload or a shared link loads them.
  await page.goto(`${API}/sessions/20260928T020401.195Z-14715`)
  await expect(page.getByRole('heading', { name: '20260928T020401.195Z-14715' })).toBeVisible()
  await page.goto(`${API}/flows/shop.promoted`)
  await expect(page.getByRole('heading', { name: 'shop.promoted' })).toBeVisible()
})
