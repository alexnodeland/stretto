// Shared by the video renderers: find ffmpeg, open a page in headless
// Chromium, capture frames, and pipe them into ffmpeg's stdin. No frame is
// written to disk. The voice-over (narrate.py's clips) is mixed into one
// track, muxed with the frames, and written out as WebVTT captions.
//
// ffmpeg is $FFMPEG, else `ffmpeg` on PATH, else the binary of the Python
// package imageio-ffmpeg ($PYTHON, else python3). Chromium is Playwright's,
// or $CHROMIUM_PATH.
import { chromium } from 'playwright';
import { spawn, execFileSync } from 'node:child_process';
import { once } from 'node:events';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

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
// `voice`, when given, is window.__voice before the page's scripts run, so a
// timeline can make room for each spoken line. Returns the page and frame(t),
// which draws t with window.__render and returns the frame as PNG bytes, once
// it is on screen.
export async function openPage(browser, url, prepare = async () => {}, voice = null) {
  const page = await browser.newPage({ viewport: { width: 1920, height: 1080 }, deviceScaleFactor: 1 });
  page.on('pageerror', e => { console.error(e); process.exitCode = 1; });
  if (voice) await page.addInitScript(v => { window.__voice = v; }, voice);
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

// Pipe frames for t in [from, to) at fps into ffmpeg, with its output
// arguments. `audio` is a sound file to mux in as the second input (see
// h264Args).
export async function encode({ frame, fps, from = 0, to, outArgs, label, audio = null }) {
  const n = Math.round((to - from) * fps);
  const ff = spawn(ffmpegPath(), ['-y', '-hide_banner', '-loglevel', 'error', '-f', 'image2pipe', '-framerate', String(fps),
    '-c:v', 'png', '-i', '-', ...(audio ? ['-i', audio] : []), ...outArgs], { stdio: ['pipe', 'inherit', 'inherit'] });
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
// With `audio`, the second input is muxed in as AAC; else the file is silent.
export function h264Args(out, fps, crf = 18, audio = false) {
  return [
    '-map', '0:v', ...(audio ? ['-map', '1:a', '-c:a', 'aac', '-b:a', '128k', '-ar', '48000', '-ac', '2'] : ['-an']),
    '-vf', 'scale=out_color_matrix=bt709:out_range=tv,format=yuv420p',
    '-c:v', 'libx264', '-preset', 'slow', '-tune', 'animation', '-crf', String(crf),
    '-profile:v', 'high', '-level', '4.1', '-g', String(fps * 2),
    '-color_primaries', 'bt709', '-color_trc', 'bt709', '-colorspace', 'bt709', '-color_range', 'tv',
    '-movflags', '+faststart', '-r', String(fps), out,
  ];
}

// ------------------------------------------------------------ the voice-over

// narrate.py's clips for a video, from video/out/voice/NAME/manifest.json, or
// null if they have not been made: the video is then rendered silent.
export function loadVoice(name) {
  const dir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), 'out', 'voice', name);
  const file = path.join(dir, 'manifest.json');
  if (!fs.existsSync(file)) {
    console.warn(`${name}: no voice-over (run narrate.py first); rendering it silent`);
    return null;
  }
  const manifest = JSON.parse(fs.readFileSync(file, 'utf8'));
  for (const l of manifest.lines) l.file = path.join(dir, l.file);
  return manifest;
}

// Mix `clips` ([{file, t}], in seconds) into one track `duration` long, at
// out, at -16 LUFS with true peaks under -1.5 dBTP, as spoken web video is.
// The clips never overlap: the timelines hold each line until it ends.
export function voiceTrack(clips, duration, out) {
  const inputs = clips.flatMap(c => ['-i', c.file]);
  const delays = clips.map((c, i) => `[${i}:a]aresample=48000,adelay=${Math.round(c.t * 1000)}:all=1[a${i}]`);
  const mix = `${clips.map((_, i) => `[a${i}]`).join('')}amix=inputs=${clips.length}:normalize=0:dropout_transition=0,`
    + `loudnorm=I=-16:TP=-1.5:LRA=11,aresample=48000,`
    + `apad=whole_dur=${duration.toFixed(3)},atrim=0:${duration.toFixed(3)}[out]`;
  execFileSync(ffmpegPath(), ['-y', '-hide_banner', '-loglevel', 'error', ...inputs,
    '-filter_complex', [...delays, mix].join(';'), '-map', '[out]', '-ac', '2', '-c:a', 'pcm_s16le', out]);
}

// A caption's pieces: its sentences, and a sentence longer than `max`
// characters split again at the clause break nearest its middle, if that
// leaves both halves long enough to read.
export function cuePieces(text, max = 84) {
  const split = s => {
    if (s.length <= max) return [s];
    // A break leaving a piece too short to read is no break.
    const breaks = [...s.matchAll(/[,:;] /g)].map(m => m.index + 1).filter(i => i >= 24 && s.length - i >= 24);
    if (!breaks.length) return [s];
    const at = breaks.reduce((best, i) => (Math.abs(i - s.length / 2) < Math.abs(best - s.length / 2) ? i : best));
    return [...split(s.slice(0, at).trim()), ...split(s.slice(at).trim())];
  };
  return text.split(/(?<=[.!?])\s+/).map(p => p.trim()).filter(Boolean).flatMap(split);
}

// Put `audio` into the video at `file` in place of its sound, copying its
// frames: for a change to the voice-over alone (render.mjs --audio-only).
export function remux(file, audio) {
  const tmp = `${file}.remux.mp4`;
  execFileSync(ffmpegPath(), ['-y', '-hide_banner', '-loglevel', 'error', '-i', file, '-i', audio,
    '-map', '0:v', '-map', '1:a', '-c:v', 'copy', '-c:a', 'aac', '-b:a', '128k', '-ar', '48000', '-ac', '2',
    '-movflags', '+faststart', '-shortest', tmp]);
  fs.renameSync(tmp, file);
}

// Captions for the voice-over, as WebVTT: each line split into pieces (see
// cuePieces), each piece given its share of the line's time by length.
export function writeVtt(lines, out) {
  const stamp = s => {
    const ms = Math.max(0, Math.round(s * 1000));
    const h = Math.floor(ms / 3600000), m = Math.floor(ms / 60000) % 60, sec = Math.floor(ms / 1000) % 60;
    return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}:${String(sec).padStart(2, '0')}.${String(ms % 1000).padStart(3, '0')}`;
  };
  const cues = [];
  for (const { text, t, duration } of lines) {
    const parts = cuePieces(text);
    const total = parts.reduce((a, p) => a + p.length, 0);
    let at = t;
    for (const p of parts) {
      const d = (duration * p.length) / total;
      cues.push(`${stamp(at)} --> ${stamp(at + d)}\n${p}`);
      at += d;
    }
  }
  fs.writeFileSync(out, `WEBVTT\n\n${cues.map((c, i) => `${i + 1}\n${c}`).join('\n\n')}\n`);
}

// Each word of `text` with a time from the voice's own words ([{w, start,
// end}], as a speech aligner heard them). The two lists are aligned by edit
// distance on their letters and digits, so a word the aligner misheard
// ("Strato" for "stretto", "30%" for "thirty percent") still takes its place,
// and a word it did not hear at all is placed between its neighbours.
export function alignWords(text, words) {
  const norm = w => String(w).toLowerCase().normalize('NFKD').replace(/[^a-z0-9]/g, '');
  const A = text.split(/\s+/).filter(Boolean);
  const a = A.map(norm), b = words.map(w => norm(w.w ?? w.word ?? ''));
  const n = a.length, m = b.length;
  const lev = (x, y) => {
    let prev = Array.from({ length: y.length + 1 }, (_, j) => j);
    for (let i = 1; i <= x.length; i++) {
      const cur = [i];
      for (let j = 1; j <= y.length; j++) cur[j] = Math.min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (x[i - 1] === y[j - 1] ? 0 : 1));
      prev = cur;
    }
    return prev[y.length];
  };
  const sub = (x, y) => (x === y ? 0 : !x || !y ? 1 : Math.min(1, lev(x, y) / Math.max(x.length, y.length)));
  const GAP = 0.8;
  const D = Array.from({ length: n + 1 }, (_, i) => Float64Array.from({ length: m + 1 }, (_, j) => (i + j) * GAP));
  for (let i = 1; i <= n; i++) for (let j = 1; j <= m; j++)
    D[i][j] = Math.min(D[i - 1][j - 1] + sub(a[i - 1], b[j - 1]), D[i - 1][j] + GAP, D[i][j - 1] + GAP);
  const at = new Array(n).fill(-1);
  for (let i = n, j = m; i > 0 && j > 0;) {
    if (Math.abs(D[i][j] - (D[i - 1][j - 1] + sub(a[i - 1], b[j - 1]))) < 1e-9) { at[i - 1] = j - 1; i--; j--; }
    else if (Math.abs(D[i][j] - (D[i - 1][j] + GAP)) < 1e-9) i--;
    else j--;
  }
  const end = m ? +words[m - 1].end : 0;
  return A.map((w, i) => {
    if (at[i] >= 0) return { w, start: +words[at[i]].start, end: +words[at[i]].end };
    // Between the nearest aligned words on either side.
    let p = i - 1, q = i + 1;
    while (p >= 0 && at[p] < 0) p--;
    while (q < n && at[q] < 0) q++;
    const t0 = p >= 0 ? +words[at[p]].end : 0, t1 = q < n ? +words[at[q]].start : end;
    const k = (i - p) / (q - p);
    return { w, start: t0 + (t1 - t0) * k, end: t0 + (t1 - t0) * Math.min(1, k + 1 / (q - p)) };
  });
}

// Captions as writeVtt makes them, but timed by the voice's own words where
// a line has them ([{w, start, end}], from its clip's start): each piece
// starts when its first word is spoken, and holds until the next piece. A
// line without word timings, or a piece whose words are not found, gets its
// share of the line by length, as writeVtt gives it.
export function writeVttWords(lines, out) {
  const stamp = s => {
    const ms = Math.max(0, Math.round(s * 1000));
    const h = Math.floor(ms / 3600000), m = Math.floor(ms / 60000) % 60, sec = Math.floor(ms / 1000) % 60;
    return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}:${String(sec).padStart(2, '0')}.${String(ms % 1000).padStart(3, '0')}`;
  };
  const cues = [];
  for (const { text, t, duration, words } of lines) {
    const parts = cuePieces(text);
    const total = parts.reduce((a, p) => a + p.length, 0);
    let acc = 0;
    const starts = parts.map(p => { const at = t + (duration * acc) / total; acc += p.length; return at; });
    if (words && words.length) {
      // Each piece starts when its first word is spoken (see alignWords).
      const timed = alignWords(text, words);
      let k = 0;
      parts.forEach((p, i) => { starts[i] = t + timed[Math.min(k, timed.length - 1)].start; k += p.split(/\s+/).filter(Boolean).length; });
    }
    for (let i = 1; i < starts.length; i++) starts[i] = Math.max(starts[i], starts[i - 1] + 0.5);
    parts.forEach((p, i) => cues.push(`${stamp(starts[i])} --> ${stamp(i + 1 < parts.length ? starts[i + 1] : t + duration)}\n${p}`));
  }
  fs.writeFileSync(out, `WEBVTT\n\n${cues.map((c, i) => `${i + 1}\n${c}`).join('\n\n')}\n`);
}

export function report(file, root) {
  console.log(`${path.relative(root, file)}: ${(fs.statSync(file).size / 1e6).toFixed(2)} MB`);
}

export function option(args, name, dflt) {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : dflt;
}
