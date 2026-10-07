// Capture the console film's shots from the real stretto-console, driven as a
// user drives it: each shot is a page or a state after a click, saved at twice
// the size it is drawn (1440 x 900, device scale 2) in the dark theme, with
// the boxes (in page pixels) of what its narration points at, for the film's
// rings and cursor. capture.sh starts the console over a re-dated copy of its
// test fixtures and runs this; it writes shots/*.png, shots/shots.json and
// shots/shots.js (the same, for the film's page).
//
//   node video/console/capture.mjs [--url http://127.0.0.1:7891] [--read-only-url http://127.0.0.1:7892]
//
// The film's address bar shows 127.0.0.1:7878, the console's default address.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { launchBrowser, option } from '../lib.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const URL0 = option(args, '--url', 'http://127.0.0.1:7891');
const URL_RO = option(args, '--read-only-url', 'http://127.0.0.1:7892');
const out = path.join(here, 'shots');
fs.mkdirSync(out, { recursive: true });

const browser = await launchBrowser();
const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2, colorScheme: 'dark', reducedMotion: 'reduce' });
await context.addInitScript(() => { try { localStorage.setItem('stretto-console:theme', 'dark'); localStorage.removeItem('stretto-console:domain'); } catch {} });
const page = await context.newPage();
page.on('pageerror', e => { console.error(e); process.exitCode = 1; });

const shots = [];
const box = async loc => {
  const b = await loc.first().boundingBox();
  if (!b) throw new Error('capture: a box to point at is not on the page');
  return [Math.round(b.x), Math.round(b.y), Math.round(b.width), Math.round(b.height)];
};
async function go(url, base = URL0) {
  await page.goto(base + url, { waitUntil: 'load' });
  await page.locator('h1').first().waitFor();
  await page.waitForTimeout(700);
}
/** Save the page as shot `id`, with named boxes of what the film points at. */
async function shot(id, url, boxes = {}) {
  await page.mouse.move(1439, 899); // no hover state in the picture
  await page.waitForTimeout(300);
  const rects = {};
  for (const [k, loc] of Object.entries(boxes)) rects[k] = await box(loc);
  await page.screenshot({ path: path.join(out, `${id}.png`) });
  shots.push({ id, url, boxes: rects });
  console.log(`capture: ${id} ${url}`);
}
const text = (t, root = page) => root.getByText(t, { exact: false });

// The overview, and its domains.
await go('/');
await shot('overview', '127.0.0.1:7878', { totals: page.getByRole('region', { name: 'Totals' }), chart: page.locator('figure').first(), nav: page.getByRole('navigation', { name: 'Pages' }) });
await page.getByRole('heading', { name: 'Domains' }).scrollIntoViewIfNeeded();
await page.evaluate(() => window.scrollBy(0, 260));
await page.waitForTimeout(400);
await shot('domains', '127.0.0.1:7878', { domains: page.locator('article.dc').first() });

// Servers: the registry, a server's host configuration, and its connection test.
await go('/servers');
await shot('servers', '127.0.0.1:7878/servers', { shop: page.getByTestId('server-shop') });
await go('/servers/shop');
await shot('server', '127.0.0.1:7878/servers/shop', { snippet: page.getByTestId('host-snippet'), tabs: page.getByRole('tablist').first(), probe: page.getByTestId('probe') });
await page.getByTestId('probe').click();
await page.getByTestId('probe-ok').waitFor({ timeout: 30_000 });
await page.locator('table').last().scrollIntoViewIfNeeded();
await page.evaluate(() => window.scrollBy(0, 140));
await page.waitForTimeout(500);
await shot('probe', '127.0.0.1:7878/servers/shop', { ok: page.getByTestId('probe-ok'), tools: page.locator('table').last() });

// Sessions: the list, a served session's timeline, its decisions, a shadow session.
await go('/sessions');
await shot('sessions', '127.0.0.1:7878/sessions', { modes: page.getByRole('radiogroup').first(), table: page.locator('.st-table'), served: page.getByRole('link', { name: '20260928T020401.195Z-14715' }) });
const served = '/sessions/20260928T020401.195Z-14715';
await go(served);
await shot('session', `127.0.0.1:7878${served}`, { stats: text('Tool calls').first().locator('xpath=ancestor::*[contains(@class,"stats") or self::section][1]'), lookups: text('Read ahead by stretto: 3 lookups'), score: text('0.99 × 0.88 binding'), decisionsTab: page.getByRole('tab', { name: /Decisions/ }) });
await go(`${served}?tab=decisions`);
await shot('decisions', `127.0.0.1:7878${served}?tab=decisions`, { table: page.locator('table.dt'), handback: text('get_order_details at 0.05, below 0.3') });
await go('/sessions/20260928T020401.278Z-14768');
const shadow = text('In shadow').nth(1);
await shadow.scrollIntoViewIfNeeded().catch(() => {});
await page.waitForTimeout(300);
await shot('shadow', '127.0.0.1:7878/sessions/20260928T020401.278Z-14768', { badge: text('Shadow').first(), would: page.getByTestId('decision-shadow') });

// Flows: the graph, the threshold moved, the review, the comparison.
await go('/flows/shop');
await shot('flow', '127.0.0.1:7878/flows/shop', { graph: page.getByTestId('flow-graph'), slider: page.getByTestId('threshold-range'), acting: page.getByTestId('acting'), reviewTab: page.getByRole('tab', { name: 'Review' }), compareTab: page.getByRole('tab', { name: 'Compare' }) });
await go('/flows/shop?threshold=0.50');
await page.waitForTimeout(400);
await shot('flow50', '127.0.0.1:7878/flows/shop?threshold=0.50', { graph: page.getByTestId('flow-graph'), slider: page.getByTestId('threshold-range'), acting: page.getByTestId('acting') });
await go('/flows/shop?tab=review');
await shot('review', '127.0.0.1:7878/flows/shop?tab=review', { review: text('Flow: shop').first() });
await go('/flows/shop?tab=compare&compare=shop.promoted');
await shot('compare', '127.0.0.1:7878/flows/shop?tab=compare&compare=shop.promoted', { verdict: text('Nothing needs review').first() });

// Jobs: an audit, started from the page, and its report.
await go('/jobs/new?kind=audit&flow=shop');
await page.getByPlaceholder('logs/shop').fill('served/shop');
await page.waitForTimeout(300);
await shot('jobnew', '127.0.0.1:7878/jobs/new?kind=audit', { kinds: text('Learn a flow from recorded sessions').locator('xpath=ancestor::*[@role="radiogroup" or contains(@class,"kinds")][1]'), start: page.getByTestId('job-start'), command: text('stretto audit --flow').first() });
await page.getByTestId('job-start').click();
await page.getByText('Succeeded').first().waitFor({ timeout: 60_000 });
await page.getByRole('heading', { name: 'Flow audit: shop' }).waitFor();
await page.waitForTimeout(500);
const jobUrl = new URL(page.url()).pathname;
await shot('job', `127.0.0.1:7878${jobUrl}`, { status: text('Succeeded').first(), report: page.getByRole('heading', { name: 'Flow audit: shop' }) });

// The command palette.
await go('/');
await page.keyboard.press('Control+k');
await page.getByTestId('palette-input').waitFor();
await page.getByTestId('palette-input').fill('shop');
await page.waitForTimeout(500);
await shot('palette', '127.0.0.1:7878', { palette: page.getByRole('listbox', { name: 'Results' }), input: page.getByTestId('palette-input') });

// Read-only: a second console, started with --read-only, over the same data.
await go('/servers/shop', URL_RO);
await shot('readonly', '127.0.0.1:7878/servers/shop', { badge: page.getByTestId('read-only') });

fs.writeFileSync(path.join(out, 'shots.json'), JSON.stringify({ size: [1440, 900], scale: 2, shots }, null, 1) + '\n');
// The same, as a script the film's page loads (a page opened from a file cannot fetch JSON).
fs.writeFileSync(path.join(out, 'shots.js'), `// Written by capture.mjs.\nwindow.SHOTS = ${JSON.stringify({ size: [1440, 900], scale: 2, shots })};\n`);
await browser.close();
console.log(`capture: ${shots.length} shots in ${path.relative(process.cwd(), out)}`);
