import path from 'node:path'
import type { Page } from '@playwright/test'
import { test, mockState, sessionKey } from './fixtures'

const dir = process.env.SHOTS_DIR ?? path.join('test-results', 'shots')
const widths = [1440, 390]
const themes = ['light', 'dark'] as const

interface Shot {
  name: string
  path: string | (() => Promise<string>)
  state?: Record<string, unknown>
  /** Something to do on the page before the picture. */
  act?: (page: Page) => Promise<void>
}

test('every page, in light and dark, wide and narrow', async ({ page, browser, baseURL }) => {
  const served = await sessionKey(page, 'served', 'shop')
  const shadow = await sessionKey(page, 'shadow')
  const shots: Shot[] = [
    { name: 'overview', path: '/' },
    { name: 'overview-first-run', path: '/', state: { empty: true } },
    { name: 'servers', path: '/servers' },
    {
      name: 'server',
      path: '/servers/shop',
      act: async (p) =>
        void (await p.getByTestId('probe').click(), await p.getByTestId('probe-ok').waitFor()),
    },
    { name: 'server-new', path: '/servers/new?domain=tickets' },
    { name: 'sessions', path: '/sessions' },
    { name: 'session', path: `/sessions/${served}` },
    { name: 'session-shadow', path: `/sessions/${shadow}` },
    { name: 'session-decisions', path: `/sessions/${served}?tab=decisions` },
    { name: 'session-raw', path: `/sessions/${served}?tab=raw` },
    { name: 'flows', path: '/flows' },
    { name: 'flow', path: '/flows/retail' },
    { name: 'flow-promoted', path: '/flows/shop-promoted?threshold=0.50' },
    { name: 'flow-review', path: '/flows/shop?tab=review' },
    { name: 'flow-compare', path: '/flows/retail?tab=compare&compare=shop' },
    { name: 'jobs', path: '/jobs' },
    { name: 'job-new', path: '/jobs/new?kind=promote&flow=shop' },
    { name: 'job', path: '/jobs/j-0005' },
    { name: 'settings', path: '/settings' },
    {
      name: 'palette',
      path: '/',
      act: async (p) =>
        void (await p.keyboard.press('Control+k'),
        await p.getByTestId('palette-input').fill('shop')),
    },
    { name: 'read-only', path: '/servers', state: { read_only: true } },
    { name: 'sign-in', path: '/', state: { auth: true } },
    { name: 'not-found', path: '/nowhere' },
  ]
  for (const theme of themes) {
    for (const width of widths) {
      // A page of its own for each pass: a dev server's page loads a hundred
      // modules, and one page loaded a hundred times runs out of resources.
      const shotPage = await browser.newPage({
        baseURL,
        colorScheme: theme,
        viewport: { width, height: width > 800 ? 900 : 844 },
      })
      for (const shot of shots) {
        await mockState(shotPage, { reset: true, latency: 0, ...(shot.state ?? {}) })
        const url = typeof shot.path === 'string' ? shot.path : await shot.path()
        await shotPage.goto(url)
        await shotPage.locator('h1').first().waitFor()
        await shotPage.waitForLoadState('load')
        await shotPage.waitForTimeout(700)
        if (shot.act) await shot.act(shotPage)
        await shotPage.waitForTimeout(250)
        const full = shot.name !== 'palette'
        await shotPage.screenshot({
          path: path.join(dir, `${shot.name}-${theme}-${width}.png`),
          fullPage: full,
        })
      }
      await shotPage.close()
    }
  }
})
