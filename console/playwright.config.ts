import { defineConfig, devices } from '@playwright/test'

// End-to-end tests against `vite --mode mock`: the UI and the mock API in ./mock.
// The tests share the mock's state, so they run one at a time and reset it first.
const PORT = Number(process.env.E2E_PORT ?? 5188)

export default defineConfig({
  testDir: 'e2e',
  testIgnore: ['shots.spec.ts', 'real/**'],
  fullyParallel: false,
  workers: 1,
  retries: process.env.CI ? 1 : 0,
  timeout: 30_000,
  expect: { timeout: 7_000 },
  reporter: process.env.CI ? [['list'], ['html', { open: 'never' }]] : [['list']],
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    trace: 'retain-on-failure',
    colorScheme: 'light',
  },
  webServer: {
    command: `npx vite --mode mock --port ${PORT} --strictPort`,
    url: `http://127.0.0.1:${PORT}/api/health`,
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
    env: {
      MOCK_TRAFFIC: '0',
      MOCK_LATENCY: '20',
      MOCK_JOB_STEP_MS: '60',
      VITE_CONFIG_NATIVE_IGNORE_WARNING: 'true',
    },
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
})
