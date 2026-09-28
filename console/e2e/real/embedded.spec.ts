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
