import { defineConfig, devices } from '@playwright/test'

// `npm run e2e:real`: end-to-end tests against the real stretto-console, on a
// copy of its test fixtures (crates/stretto-console/tests/fixtures/home).
// e2e/real/serve.sh starts the server from the binaries cargo built;
// `vite preview` serves the built UI (console/dist) under the server's
// Content-Security-Policy and passes /api on to it. The tests share the data
// directory, so they run one at a time, in order.
const PORT = Number(process.env.E2E_REAL_PORT ?? 5189)
const API_PORT = Number(process.env.E2E_API_PORT ?? 7891)

export default defineConfig({
  testDir: 'e2e/real',
  fullyParallel: false,
  workers: 1,
  retries: 0,
  timeout: 45_000,
  expect: { timeout: 10_000 },
  reporter: process.env.CI ? [['list'], ['html', { open: 'never' }]] : [['list']],
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    trace: 'retain-on-failure',
    colorScheme: 'light',
  },
  webServer: [
    {
      command: 'sh e2e/real/serve.sh',
      url: `http://127.0.0.1:${API_PORT}/api/health`,
      // A fresh copy of the fixtures each run: jobs and deletes change it.
      reuseExistingServer: false,
      timeout: 60_000,
      env: { E2E_API_PORT: String(API_PORT) },
    },
    {
      command: `npx vite preview --port ${PORT} --strictPort`,
      url: `http://127.0.0.1:${PORT}/`,
      reuseExistingServer: false,
      timeout: 60_000,
      env: {
        STRETTO_CONSOLE_URL: `http://127.0.0.1:${API_PORT}`,
        VITE_CONFIG_NATIVE_IGNORE_WARNING: 'true',
      },
    },
  ],
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
})
