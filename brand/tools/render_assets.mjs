// Render the brand kit's raster files with headless Chromium (Playwright):
//
//   node brand/tools/render_assets.mjs            # icons and the social card
//   node brand/tools/render_assets.mjs icons      # only the icons
//   node brand/tools/render_assets.mjs og         # only the social card
//
// logo/favicon.svg -> logo/favicon.ico (16, 32 and 48 px, PNG-compressed entries)
// logo/app-icon.svg -> logo/apple-touch-icon.png (180 px), logo/icon-512.png
// social/og-card.html -> social/og-card.png (1200 x 630)
//
// Set CHROMIUM_PATH to use a Chromium other than Playwright's own.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { chromium } from 'playwright';

const brand = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const what = process.argv[2] || 'all';

const browser = await chromium.launch(process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {});

async function svgToPng(svgFile, size) {
  const page = await browser.newPage({ viewport: { width: size, height: size }, deviceScaleFactor: 1 });
  const svg = fs.readFileSync(svgFile, 'utf8');
  const uri = 'data:image/svg+xml;base64,' + Buffer.from(svg).toString('base64');
  await page.setContent(`<!doctype html><style>html,body{margin:0;background:transparent}img{display:block}</style><img src="${uri}" width="${size}" height="${size}">`);
  await page.evaluate(() => document.querySelector('img').decode());
  const png = await page.screenshot({ omitBackground: true, clip: { x: 0, y: 0, width: size, height: size } });
  await page.close();
  return png;
}

// An ICO file whose entries are PNG images (supported by every current browser and Windows since Vista).
function ico(entries) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(entries.length, 4);
  const dir = Buffer.alloc(16 * entries.length);
  let offset = 6 + dir.length;
  entries.forEach(({ size, png }, i) => {
    const o = i * 16;
    dir.writeUInt8(size >= 256 ? 0 : size, o);
    dir.writeUInt8(size >= 256 ? 0 : size, o + 1);
    dir.writeUInt8(0, o + 2);
    dir.writeUInt8(0, o + 3);
    dir.writeUInt16LE(1, o + 4);
    dir.writeUInt16LE(32, o + 6);
    dir.writeUInt32LE(png.length, o + 8);
    dir.writeUInt32LE(offset, o + 12);
    offset += png.length;
  });
  return Buffer.concat([header, dir, ...entries.map(e => e.png)]);
}

function report(file) {
  console.log(`${path.relative(path.dirname(brand), file)}: ${fs.statSync(file).size} bytes`);
}

if (what === 'all' || what === 'icons') {
  const logo = path.join(brand, 'logo');
  const entries = [];
  for (const size of [16, 32, 48]) entries.push({ size, png: await svgToPng(path.join(logo, 'favicon.svg'), size) });
  fs.writeFileSync(path.join(logo, 'favicon.ico'), ico(entries));
  report(path.join(logo, 'favicon.ico'));
  for (const [name, size] of [['apple-touch-icon.png', 180], ['icon-512.png', 512]]) {
    fs.writeFileSync(path.join(logo, name), await svgToPng(path.join(logo, 'app-icon.svg'), size));
    report(path.join(logo, name));
  }
}

if (what === 'all' || what === 'og') {
  const page = await browser.newPage({ viewport: { width: 1200, height: 630 }, deviceScaleFactor: 1 });
  await page.goto(pathToFileURL(path.join(brand, 'social', 'og-card.html')).href);
  await page.evaluate(() => document.fonts.ready);
  await page.evaluate(() => Promise.all([...document.images].map(i => i.decode())));
  const out = path.join(brand, 'social', 'og-card.png');
  await page.screenshot({ path: out, clip: { x: 0, y: 0, width: 1200, height: 630 } });
  report(out);
}

await browser.close();
