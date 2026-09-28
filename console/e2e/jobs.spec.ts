import { test, expect, mockState } from './fixtures'

test('a learn job runs, streams its output, and links the flow it wrote', async ({ page }) => {
  await page.goto('/jobs/new?kind=learn')
  await page.getByTestId('learn-domain').fill('notes')
  await expect(
    page.getByText(
      'stretto learn --sessions logs/notes --domain notes --habit-only --out notes.flow.json',
    ),
  ).toBeVisible()
  await page.getByTestId('job-start').click()
  await expect(page).toHaveURL(/\/jobs\/j-\d+/)
  await expect(page.getByTestId('job-output')).toContainText('$ stretto learn')
  await expect(page.getByText('Succeeded')).toBeVisible()
  await expect(page.getByTestId('job-output')).toContainText(
    'learned the notes flow from 12 sessions',
  )
  await page.getByTestId('job-artifacts').getByRole('link', { name: 'notes.flow.json' }).click()
  await expect(page).toHaveURL(/\/flows\/notes/)
})

test('a learn job refuses to write over a flow without leave', async ({ page }) => {
  await page.goto('/jobs/new?kind=learn&domain=shop')
  await page.getByTestId('job-start').click()
  await expect(page.getByRole('alert')).toContainText('shop.flow.json exists')
})

test('a failed job says so, with its output', async ({ page }) => {
  await page.goto('/jobs')
  await page.getByTestId('job-j-0004').getByRole('link').first().click()
  await expect(page.getByText('Failed').first()).toBeVisible()
  await expect(page.getByTestId('job-output')).toContainText('no successful training episodes')
})

test('an audit’s report is rendered', async ({ page }) => {
  await page.goto('/jobs/j-0005')
  await expect(page.getByRole('heading', { name: 'Flow audit: shop' })).toBeVisible()
  await page.getByRole('radio', { name: 'Output' }).click()
  await expect(page.getByTestId('job-output')).toContainText('stretto: audited 32 decisions')
})

test('redacting needs a salt in the console’s environment', async ({ page }) => {
  await page.goto('/jobs/new?kind=redact')
  await page.getByPlaceholder('logs/shop').fill('logs/shop')
  await page.getByTestId('job-start').click()
  await expect(page.getByRole('alert')).toContainText('STRETTO_REDACT_SALT')
  await mockState(page, { redact_salt: true })
  await page.getByTestId('job-start').click()
  await expect(page).toHaveURL(/\/jobs\/j-\d+/)
  await expect(page.getByTestId('job-artifacts')).toContainText('redacted/shop')
})

test('a job can be cancelled while it runs, or before it starts', async ({ page }) => {
  // Slow jobs: two seconds in the queue, and between the lines they print.
  await mockState(page, { job_step_ms: 2000 })
  await page.goto('/jobs/new?kind=doctor')
  await page.getByTestId('job-start').click()
  await expect(page).toHaveURL(/\/jobs\/j-\d+/)
  const heading = page.getByRole('heading', { level: 1 })
  const output = page.getByTestId('job-output')
  const cancel = page.getByRole('button', { name: 'Cancel', exact: true })

  // Queued: it never runs.
  await expect(heading).toContainText('Queued')
  await cancel.click()
  await expect(heading).toContainText('Cancelled')
  await expect(output).toHaveText('stretto-console: cancelled before it started')
  await expect(page.getByText('Check the installation: cancelled')).toBeVisible()
  const first = page.url()

  // Running: its stretto is killed, and the job ends a moment later.
  await page.getByRole('button', { name: 'Run again' }).click()
  await expect(page).not.toHaveURL(first)
  await expect(heading).toContainText('Running', { timeout: 5000 })
  await cancel.click()
  await expect(heading).toContainText('Cancelled')
  await expect(output).toContainText('$ stretto doctor')
  await expect(output).toContainText('stretto-console: cancelled')
  await expect(output).not.toContainText('before it started')
  await expect(page.getByText('exit code 137')).toBeVisible()
  await expect(cancel).toHaveCount(0)
  await expect(page.getByRole('button', { name: 'Run again' })).toBeEnabled()
})

test('the API refuses to cancel a job that has ended', async ({ page }) => {
  const response = await page.request.post('/api/jobs/j-0001/cancel', {
    headers: { 'X-Stretto-Console': '1' },
  })
  expect(response.status()).toBe(409)
  expect((await response.json()).error).toContain('has ended')
})

test('run again on a learn job opens the form with its parameters, to replace the flow', async ({
  page,
}) => {
  await page.goto('/jobs/j-0002')
  await page.getByRole('button', { name: 'Run again…' }).click()
  await expect(page).toHaveURL(/\/jobs\/new\?.*from=j-0002/)
  await expect(page.getByTestId('job-from')).toContainText('Learn shop from logs/shop')
  await expect(page.getByTestId('learn-domain')).toHaveValue('shop')
  await expect(page.getByPlaceholder('logs/shop')).toHaveValue('logs/shop')
  // The flow the first run wrote is there: the same request is refused...
  await page.getByTestId('job-start').click()
  await expect(page.getByRole('alert')).toContainText('shop.flow.json exists')
  // ...until it may replace it.
  await page.getByRole('switch', { name: /Replace an existing flow/ }).click()
  await page.getByTestId('job-start').click()
  await expect(page).toHaveURL(/\/jobs\/j-\d+$/)
  await expect(page.getByRole('heading', { level: 1 })).toContainText('Succeeded', {
    timeout: 10_000,
  })
})

test('run again on a promote job keeps its thresholds and its path', async ({ page }) => {
  await page.goto('/jobs/j-0003')
  await page.getByRole('button', { name: 'Run again…' }).click()
  await expect(page.getByTestId('job-from')).toContainText('Promote shop on shadow/shop')
  await expect(page.getByLabel('Write the promoted flow to')).toHaveValue('shop-promoted.flow.json')
  await expect(page.getByLabel('Least share used')).toHaveValue('0.7')
  await expect(page.getByRole('switch', { name: /Replace an existing flow/ })).toHaveAttribute(
    'aria-checked',
    'false',
  )
})
