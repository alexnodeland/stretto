// Shared by the video renderers: find ffmpeg, open a page in headless
// Chromium, capture frames, and pipe them into ffmpeg's stdin. No frame is
// written to disk.
//
// ffmpeg is $FFMPEG, else `ffmpeg` on PATH, else the binary of the Python
// package imageio-ffmpeg ($PYTHON, else python3). Chromium is Playwright's,
// or $CHROMIUM_PATH.
import { chromium } from 'playwright';
import { spawn, execFileSync } from 'node:child_process';
import { once } from 'node:events';
import fs from 'node:fs';
import path from 'node:path';

export function ffmpegPath() {
  if (process.env.FFMPEG) return process.env.FFMPEG;
  try { execFileSync('ffmpeg', ['-version'], { stdio: 'ignore' }); return 'ffmpeg'; } catch {}
  for (const py of [process.env.PYTHON, 'python3', 'python'].filter(Boolean)) {
    try { return execFileSync(py, ['-c', 'import imageio_ffmpeg as i; print(i.get_ffmpeg_exe())']).toString().trim(); } catch {}
  }
  throw new Error('no ffmpeg: set FFMPEG, put ffmpeg on PATH, or pip install imageio-ffmpeg');
}

export async function launchBrowser() {
  return chromium.launch(process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {});
}

// Open url at 1920 x 1080; `prepare(page)` runs once the page is ready.
// Returns the page and frame(t), which draws t with window.__render and
// returns the frame as PNG bytes, once it is on screen.
export async function openPage(browser, url, prepare = async () => {}) {
  const page = await browser.newPage({ viewport: { width: 1920, height: 1080 }, deviceScaleFactor: 1 });
  page.on('pageerror', e => { console.error(e); process.exitCode = 1; });
  await page.goto(url);
  await page.evaluate(() => window.__ready);
  await prepare(page);
  const cdp = await page.context().newCDPSession(page);
  const frame = async t => {
    await page.evaluate(t => new Promise(r => { window.__render(t); requestAnimationFrame(() => requestAnimationFrame(r)); }), t);
    const { data } = await cdp.send('Page.captureScreenshot', { format: 'png', optimizeForSpeed: true });
    return Buffer.from(data, 'base64');
  };
  return { page, frame };
}

// Pipe frames for t in [from, to) at fps into ffmpeg, with its output arguments.
export async function encode({ frame, fps, from = 0, to, outArgs, label }) {
  const n = Math.round((to - from) * fps);
  const ff = spawn(ffmpegPath(), ['-y', '-hide_banner', '-loglevel', 'error', '-f', 'image2pipe', '-framerate', String(fps),
    '-c:v', 'png', '-i', '-', ...outArgs], { stdio: ['pipe', 'inherit', 'inherit'] });
  const started = Date.now();
  for (let i = 0; i < n; i++) {
    const png = await frame(from + i / fps);
    if (!ff.stdin.write(png)) await once(ff.stdin, 'drain');
    if (i % 150 === 0) process.stdout.write(`\r${label}: frame ${i}/${n} (${((Date.now() - started) / 1000).toFixed(0)} s)`);
  }
  ff.stdin.end();
  const [code] = await once(ff, 'close');
  process.stdout.write(`\r${label}: ${n} frames in ${((Date.now() - started) / 1000).toFixed(0)} s\n`);
  if (code !== 0) throw new Error(`ffmpeg exited with ${code}`);
}

// H.264 for the web: yuv420p, BT.709, faststart, keyframes every two seconds.
export function h264Args(out, fps, crf = 18) {
  return [
    '-vf', 'scale=out_color_matrix=bt709:out_range=tv,format=yuv420p',
    '-c:v', 'libx264', '-preset', 'slow', '-tune', 'animation', '-crf', String(crf),
    '-profile:v', 'high', '-level', '4.1', '-g', String(fps * 2),
    '-color_primaries', 'bt709', '-color_trc', 'bt709', '-colorspace', 'bt709', '-color_range', 'tv',
    '-movflags', '+faststart', '-an', '-r', String(fps), out,
  ];
}

export function report(file, root) {
  console.log(`${path.relative(root, file)}: ${(fs.statSync(file).size / 1e6).toFixed(2)} MB`);
}

export function option(args, name, dflt) {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : dflt;
}
