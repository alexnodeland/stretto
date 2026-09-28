import { test, expect } from './fixtures'

test('doctor runs the stretto CLI and shows its output', async ({ page }) => {
  await page.goto('/jobs/new?kind=doctor')
  await page.getByTestId('job-start').click()
  await expect(page).toHaveURL(/\/jobs\/[^/]+$/)
  await expect(page.getByText('Succeeded').first()).toBeVisible({ timeout: 30_000 })
  await expect(page.getByTestId('job-output')).toContainText('$ stretto doctor')
})

test('an audit writes a report, which the job page renders', async ({ page }) => {
  await page.goto('/jobs/new?kind=audit&flow=shop')
  await page.getByPlaceholder('logs/shop').fill('served/shop')
  await page.getByTestId('job-start').click()
  await expect(page.getByText('Succeeded').first()).toBeVisible({ timeout: 30_000 })
  await expect(page.getByRole('heading', { name: 'Flow audit: shop' })).toBeVisible()
  await page.getByRole('radio', { name: 'Output' }).click()
  await expect(page.getByTestId('job-output')).toContainText('stretto: audited 8 decisions')
  await expect(page.getByTestId('job-artifacts')).toContainText('.audit.md')
})

test('a learn job writes a flow, linked from the job', async ({ page }) => {
  await page.goto('/jobs/new?kind=learn')
  await page.getByTestId('learn-domain').fill('shop')
  await page.getByPlaceholder('logs/shop').fill('logs/shop')
  await page.getByPlaceholder('shop.flow.json').fill('e2e.flow.json')
  await page.getByTestId('job-start').click()
  await expect(page.getByText('Succeeded').first()).toBeVisible({ timeout: 60_000 })
  await page.getByTestId('job-artifacts').getByRole('link', { name: 'e2e.flow.json' }).click()
  await expect(page).toHaveURL(/\/flows\/e2e$/)
  await expect(page.getByTestId('flow-graph')).toBeVisible()
})

test('a learn job will not write over a flow without leave', async ({ page }) => {
  await page.goto('/jobs/new?kind=learn&domain=shop')
  await page.getByPlaceholder('logs/shop').fill('logs/shop')
  await page.getByTestId('job-start').click()
  await expect(page.getByRole('alert')).toContainText('shop.flow.json exists')
})

test('run again on a learn job opens the form with what it ran with', async ({ page }) => {
  const response = await page.request.get('/api/jobs')
  const { items } = (await response.json()) as {
    items: { id: string; kind: string; params: { out?: string } }[]
  }
  const learned = items.find((j) => j.kind === 'learn' && j.params.out === 'e2e.flow.json')
  expect(learned, 'the learn job that wrote e2e.flow.json').toBeTruthy()
  await page.goto(`/jobs/${learned!.id}`)
  await page.getByRole('button', { name: 'Run again…' }).click()
  await expect(page.getByTestId('learn-domain')).toHaveValue('shop')
  await expect(page.getByPlaceholder('shop.flow.json')).toHaveValue('e2e.flow.json')
  await page.getByTestId('job-start').click()
  await expect(page.getByRole('alert')).toContainText('e2e.flow.json exists')
  await page.getByRole('switch', { name: /Replace an existing flow/ }).click()
  await page.getByTestId('job-start').click()
  await expect(page.getByText('Succeeded').first()).toBeVisible({ timeout: 60_000 })
})

test('the jobs list has every job, newest first', async ({ page }) => {
  await page.goto('/jobs')
  await expect(page.getByRole('heading', { name: 'Jobs', level: 1 })).toBeVisible()
  await expect(page.getByRole('link', { name: 'Check the installation' })).toBeVisible()
  await expect(page.getByRole('link', { name: 'Audit shop on served/shop' })).toBeVisible()
})
