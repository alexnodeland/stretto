import { defineConfig } from '@playwright/test'
import base from './playwright.config.ts'

// `npm run shots`: every page in light and dark, at 1440 and 390 px wide, into
// SHOTS_DIR (default test-results/shots), for looking at the design.
export default defineConfig({
  ...base,
  testIgnore: ['real/**'],
  testMatch: ['shots.spec.ts'],
  timeout: 180_000,
  // One long test of screenshots: a trace of it would be all pictures.
  use: { ...base.use, trace: 'off' },
  projects: [{ name: 'shots' }],
})
