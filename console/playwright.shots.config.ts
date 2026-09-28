import { defineConfig } from '@playwright/test'
import base from './playwright.config.ts'

// `npm run shots`: every page in light and dark, at 1440 and 390 px wide, into
// SHOTS_DIR (default test-results/shots), for looking at the design.
export default defineConfig({
  ...base,
  testIgnore: [],
  testMatch: ['shots.spec.ts'],
  timeout: 180_000,
  projects: [{ name: 'shots' }],
})
