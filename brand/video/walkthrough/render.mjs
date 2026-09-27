// Render the walkthrough video from the recorded session.
//
//   node render.mjs                        # brand/media/walkthrough.mp4 and walkthrough-poster.png
//   node render.mjs --cast other.cast      # from another recording (default: walkthrough.cast)
//   node render.mjs --stills 12,40.5 --stills-dir /tmp/stills   # check frames, as PNG
//
// Record the session first with capture.py (see brand/README.md). term.html
// draws each frame from the cast with window.__render(t); frames go straight
// into ffmpeg's stdin.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { launchBrowser, openPage, encode, h264Args, report, option } from '../lib.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..', '..', '..');
const media = path.resolve(here, '..', '..', 'media');
const args = process.argv.slice(2);
const FPS = +option(args, '--fps', '30');
const cast = fs.readFileSync(path.resolve(option(args, '--cast', path.join(here, 'walkthrough.cast'))), 'utf8');

const browser = await launchBrowser();
const { page, frame } = await openPage(browser, pathToFileURL(path.join(here, 'term.html')).href,
  p => p.evaluate(text => window.__load(text), cast));
const duration = await page.evaluate(() => window.__duration);
const posterT = +option(args, '--poster', await page.evaluate(() => window.__posterTime));
console.log(`walkthrough: ${duration.toFixed(1)} s`);

const stills = option(args, '--stills');
if (stills) {
  const dir = option(args, '--stills-dir', path.join(here, 'out'));
  fs.mkdirSync(dir, { recursive: true });
  for (const t of stills.split(',').map(Number)) {
    const file = path.join(dir, `walkthrough-${t.toFixed(2)}.png`);
    fs.writeFileSync(file, await frame(t));
    console.log(file);
  }
} else {
  fs.mkdirSync(media, { recursive: true });
  const out = path.join(media, 'walkthrough.mp4');
  await encode({ frame, fps: FPS, from: 0, to: duration, outArgs: h264Args(out, FPS, 20), label: 'walkthrough' });
  report(out, root);
  const poster = path.join(media, 'walkthrough-poster.png');
  fs.writeFileSync(poster, await frame(posterT));
  report(poster, root);
}
await browser.close();
