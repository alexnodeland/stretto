// Render one of the brand kit's films: its video, captions and poster, and the
// explainer's teaser.
//
//   node video/film.mjs FILM                        # everything, into brand/media/ (FILM: explainer, math or console)
//   node video/film.mjs FILM --only video           # FILM.mp4, FILM.vtt, FILM-poster.png
//   node video/film.mjs explainer --only teaser     # explainer-teaser.gif and explainer-teaser.webm
//   node video/film.mjs FILM --audio-only           # the sound and captions again, into the FILM.mp4 there is
//   node video/film.mjs FILM --poster-only          # FILM-poster.png alone
//   node video/film.mjs FILM --stills 3,20.5 [--cut teaser] [--width 960] [--stills-dir DIR]
//   node video/film.mjs FILM --preview 40,52 [--cut teaser]   # a silent stretch, to video/out/preview.mp4
//   node video/film.mjs FILM --info                 # the timeline the voice sets: chapter starts, lines, anchors not found
//   options: --workers N (parallel Chromium pages, default: CPUs - 1, at most 2), --fps 30, --crf 26, --poster T
//
// A film is a directory, video/FILM/: its script (narration.json), its
// chapters (film.js) and a page (index.html) that loads them on the shared
// stage (video/stage/). Each frame is drawn by window.__render(t)
// (stage/main.js), screenshotted by headless Chromium and piped into
// ffmpeg's stdin: no frame touches the disk. Several pages draw frames at
// once and ffmpeg gets them in order.
//
// The sound: the voice-over (video/out/voice/FILM-video/, from narrate.py's
// clips), each line placed where its chapter says (window.__lines), and the
// music bed from video/music.py, called with the timeline's own times for the
// mark's three bars and the closing motif (window.__music). There are no
// sound effects (PACING.md). The music ducks under the voice
// (sidechaincompress), and the mix is normalized in two passes to -16 LUFS
// with true peaks under -1.5 dBTP. Any part that is missing is left out, with
// a warning. The captions (media/FILM.vtt) are the lines' `text`, timed by the
// voice's words.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn, execFileSync, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { launchBrowser, openPage, h264Args, report, option, loadVoice, writeVtt, writeVttWords, alignWords, remux, ffmpegPath } from './lib.mjs';

const videoDir = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(videoDir, '..', '..');
const media = path.resolve(videoDir, '..', 'media');
const out = path.join(videoDir, 'out');
const FILM = process.argv[2];
if (!FILM || FILM.startsWith('-') || !fs.existsSync(path.join(videoDir, FILM, 'narration.json'))) {
  console.error('usage: node video/film.mjs FILM [options], where FILM is a directory of video/ with a narration.json');
  process.exit(2);
}
const here = path.join(videoDir, FILM);
const args = process.argv.slice(3);
const only = option(args, '--only', 'all');
const FPS = +option(args, '--fps', '30');
const WORKERS = Math.max(1, +option(args, '--workers', String(Math.max(1, Math.min(2, os.cpus().length - 1)))));
const NAME = `${FILM}-video`;
const CRF = +option(args, '--crf', '26');

// H.264 as h264Args makes it (High, BT.709, yuv420p, faststart), at a CRF that
// keeps the file near 20 MB, with what keeps the dark gradients clean at that
// rate: an error-diffused conversion to YUV (no banding from rounding), and
// adaptive quantization that favours dark, flat areas (aq-mode 3).
function videoArgs(file, audio) {
  const a = h264Args(file, FPS, CRF, !!audio);
  a.splice(a.length - 1, 0,
    '-vf', 'zscale=rangein=full:range=limited:matrix=709:transferin=709:transfer=709:primariesin=709:primaries=709:dither=error_diffusion,format=yuv420p',
    '-preset', 'veryslow', '-x264-params', 'aq-mode=3:aq-strength=0.9:deblock=0,0');
  return a;
}

const script = JSON.parse(fs.readFileSync(path.join(here, 'narration.json'), 'utf8'));
const voice = loadVoice(NAME);
// What the page gets as window.__voice: each voiced line's length, its
// script's own words timed by the voice (alignWords, so the scenes' anchors
// are the script's words however the aligner heard them), and the script.
const scriptLines = script.beats.flatMap(b => b.lines);
const said = Object.fromEntries(scriptLines.map(l => [l.key, l.say || l.text]));
const pageVoice = {
  lines: voice ? Object.fromEntries(voice.lines.map(l => [l.key, {
    duration: l.duration,
    words: l.words && l.words.length && said[l.key] ? alignWords(said[l.key], l.words) : null,
  }])) : {},
  script,
};

const browsers = [];
async function open(cut) {
  const browser = await launchBrowser();
  browsers.push(browser);
  const url = `${pathToFileURL(path.join(here, 'index.html')).href}?cut=${cut}`;
  const opened = await openPage(browser, url, undefined, pageVoice);
  const info = await opened.page.evaluate(() => ({
    duration: window.__duration, marks: window.__marks, lines: window.__lines,
    music: window.__music, poster: window.__poster, missing: window.__missing || [], hasTeaser: !!window.__hasTeaser,
  }));
  return { ...opened, ...info };
}
async function closeAll() { for (const b of browsers.splice(0)) await b.close(); }

// Frames for t in [from, to) at fps, drawn by several pages at once, written
// in order to each sink: {outArgs, audio}.
async function encodeParallel({ pages, fps, from = 0, to, sinks, label }) {
  const n = Math.round((to - from) * fps);
  const procs = sinks.map(sk => spawn(ffmpegPath(), ['-y', '-hide_banner', '-loglevel', 'error', '-f', 'image2pipe', '-framerate', String(fps),
    '-c:v', 'png', '-i', '-', ...(sk.audio ? ['-i', sk.audio] : []), ...sk.outArgs], { stdio: ['pipe', 'inherit', 'inherit'] }));
  const slots = Array.from({ length: n }, () => { let resolve; const p = new Promise(r => { resolve = r; }); return { p, resolve }; });
  let written = 0;
  let waiting = [];
  const AHEAD = pages.length * 4;
  const started = Date.now();
  const tty = process.stdout.isTTY;
  const work = async (pg, k) => {
    for (let i = k; i < n; i += pages.length) {
      while (i - written > AHEAD) await new Promise(r => waiting.push(r));
      slots[i].resolve(await pg.frame(from + i / fps));
    }
  };
  const workers = pages.map((pg, k) => work(pg, k));
  for (let i = 0; i < n; i++) {
    const png = await slots[i].p;
    slots[i] = null;
    for (const pr of procs) if (!pr.stdin.write(png)) await once(pr.stdin, 'drain');
    written = i + 1;
    if (waiting.length) { const ws = waiting; waiting = []; ws.forEach(w => w()); }
    if (i % 150 === 0) process.stdout.write(`${tty ? '\r' : ''}${label}: frame ${i}/${n} (${((Date.now() - started) / 1000).toFixed(0)} s)${tty ? '' : '\n'}`);
  }
  await Promise.all(workers);
  for (const pr of procs) pr.stdin.end();
  const codes = await Promise.all(procs.map(pr => once(pr, 'close')));
  const secs = (Date.now() - started) / 1000;
  process.stdout.write(`${tty ? '\r' : ''}${label}: ${n} frames in ${secs.toFixed(0)} s (${(n / secs).toFixed(1)} fps)\n`);
  if (codes.some(([c]) => c !== 0)) throw new Error(`${label}: ffmpeg failed`);
  return secs;
}

// ----------------------------------------------------------------- the sound
const ff = (...a) => execFileSync(ffmpegPath(), ['-y', '-hide_banner', '-loglevel', 'error', ...a], { maxBuffer: 1 << 26 });
// Integrated loudness, in LUFS, of a file.
function loudness(file) {
  const r = spawnSync(ffmpegPath(), ['-hide_banner', '-nostats', '-i', file, '-af', 'ebur128', '-f', 'null', '-'], { encoding: 'utf8' });
  const m = [...r.stderr.matchAll(/I:\s+(-?[\d.]+) LUFS/g)].pop();
  return m ? +m[1] : null;
}

// The music bed, made to the timeline's cues; null if music.py cannot run.
function music(info) {
  const script = path.join(videoDir, 'music.py');
  if (!fs.existsSync(script)) { console.warn(`${FILM}: no video/music.py; the mix has no music`); return null; }
  const file = path.join(out, 'music', `${NAME}.wav`);
  const py = process.env.PYTHON || 'python3';
  const { open: a, end } = info.music;
  const r = spawnSync(py, [script, '--duration', info.duration.toFixed(3), '--open', a.map(x => x.toFixed(3)).join(','), '--end', end.toFixed(3), '--out', file], { encoding: 'utf8' });
  if (r.status !== 0) {
    console.warn(`${FILM}: music.py failed (${(r.stderr || r.error || '').toString().trim().split('\n').pop()}); the mix has no music. Set PYTHON to a Python with numpy and soundfile.`);
    return null;
  }
  process.stdout.write(r.stdout);
  return file;
}

const MUSIC_LUFS = -30; // the bed's loudness before ducking; the voice sits near -19.5

// Mix the voice and the music into `file`, `duration` long.
function mix({ clips, duration, musicFile, file }) {
  const inputs = [], graph = [], buses = [];
  const input = f => { inputs.push('-i', f); return inputs.length / 2 - 1; };
  const D = duration.toFixed(3);
  // The voice.
  if (clips.length) {
    clips.forEach((c, i) => {
      const k = input(c.file);
      graph.push(`[${k}:a]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,adelay=${Math.round(c.t * 1000)}:all=1[v${i}]`);
    });
    graph.push(`${clips.map((_, i) => `[v${i}]`).join('')}amix=inputs=${clips.length}:normalize=0:dropout_transition=0,apad=whole_dur=${D}[voice]`);
  }
  // The music, ducked under the voice.
  if (musicFile) {
    const k = input(musicFile);
    const L = loudness(musicFile);
    const gain = L === null ? 0.25 : Math.pow(10, (MUSIC_LUFS - L) / 20);
    graph.push(`[${k}:a]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,volume=${gain.toFixed(4)},apad=whole_dur=${D}[mus]`);
    if (clips.length) {
      graph.push('[voice]asplit=2[voice][key]');
      graph.push('[mus][key]sidechaincompress=threshold=0.04:ratio=5:attack=25:release=550:knee=4[music]');
    } else graph.push('[mus]anull[music]');
    buses.push('[music]');
  }
  if (clips.length) buses.unshift('[voice]');
  if (!buses.length) return null;
  const pre = `${file}.pre.wav`;
  graph.push(`${buses.join('')}amix=inputs=${buses.length}:normalize=0:dropout_transition=0,atrim=0:${D}[mix]`);
  ff(...inputs, '-filter_complex', graph.join(';'), '-map', '[mix]', '-ac', '2', '-ar', '48000', '-c:a', 'pcm_s24le', pre);
  // Loudness in two passes: measure, then a linear gain to -16 LUFS. True
  // peaks are held to -2 dBTP here, so they stay under -1.5 dBTP once the
  // AAC encoder has had its say (it adds a few tenths of a dB).
  const target = 'I=-16:TP=-2:LRA=11';
  const m = spawnSync(ffmpegPath(), ['-hide_banner', '-nostats', '-i', pre, '-af', `loudnorm=${target}:print_format=json`, '-f', 'null', '-'], { encoding: 'utf8' });
  const stats = JSON.parse(m.stderr.match(/\{[^{}]*"input_i"[^{}]*\}/)[0]);
  const norm = `loudnorm=${target}:measured_I=${stats.input_i}:measured_TP=${stats.input_tp}:measured_LRA=${stats.input_lra}:measured_thresh=${stats.input_thresh}:offset=${stats.target_offset}:linear=true`;
  const fade = Math.min(1, duration / 10);
  ff('-i', pre, '-af', `${norm},aresample=48000,apad=whole_dur=${D},atrim=0:${D},afade=t=out:st=${(duration - fade).toFixed(3)}:d=${fade.toFixed(3)}`, '-ac', '2', '-c:a', 'pcm_s16le', file);
  fs.rmSync(pre);
  return file;
}

// The voice clips, each where its scene says, checked not to overlap.
function placed(info) {
  if (!voice) return [];
  const clips = voice.lines.map(l => {
    if (!(l.key in info.lines)) throw new Error(`${FILM}: narration line ${l.key} names no scene`);
    return { ...l, t: info.lines[l.key].t };
  }).sort((a, b) => a.t - b.t);
  clips.forEach((c, i) => {
    const next = clips[i + 1];
    if (next && c.t + c.duration > next.t + 0.02) throw new Error(`${FILM}: line ${c.key} runs into ${next.key}`);
  });
  return clips;
}

function sound(info) {
  if (info.missing.length) console.warn(`${FILM}: phrases not found in the voice's words, placed by estimate: ${info.missing.join('; ')}`);
  const clips = placed(info);
  if (!voice) console.warn(`${FILM}: no voice-over (video/out/voice/${NAME}/manifest.json); the video has no voice`);
  const musicFile = music(info);
  const file = path.join(out, FILM, 'mix.wav');
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const track = mix({ clips, duration: info.duration, musicFile, file });
  // Captions: the written text, timed by the voice.
  if (clips.length) {
    const byKey = Object.fromEntries(scriptLines.map(l => [l.key, l]));
    const lines = clips.map(c => ({ text: byKey[c.key].text, t: c.t, duration: c.duration, words: c.words }));
    const vtt = path.join(media, `${FILM}.vtt`);
    if (lines.some(l => l.words && l.words.length)) writeVttWords(lines, vtt); else writeVtt(lines, vtt);
    report(vtt, root);
  }
  console.log(`${FILM}: sound = ${[clips.length && 'voice', musicFile && 'music'].filter(Boolean).join(' + ') || 'none'}`);
  return track;
}

// ------------------------------------------------------------------ the run
fs.mkdirSync(media, { recursive: true });
const stills = option(args, '--stills');
if (stills) {
  const dir = option(args, '--stills-dir', path.join(out, 'stills'));
  const cut = option(args, '--cut', 'video');
  const width = +option(args, '--width', '1920');
  fs.mkdirSync(dir, { recursive: true });
  const { frame } = await open(cut);
  for (const t of stills.split(',').map(Number)) {
    const file = path.join(dir, `${cut}-${t.toFixed(2)}.png`);
    const png = await frame(t);
    if (width !== 1920) {
      fs.writeFileSync(`${file}.full.png`, png);
      execFileSync(ffmpegPath(), ['-y', '-hide_banner', '-loglevel', 'error', '-i', `${file}.full.png`, '-vf', `scale=${width}:-1:flags=lanczos`, file]);
      fs.rmSync(`${file}.full.png`);
    } else fs.writeFileSync(file, png);
    console.log(file);
  }
} else if (args.includes('--info')) {
  // The timeline as the voice sets it, without rendering.
  const info = await open('video');
  const fmt = x => `${Math.floor(x / 60)}:${(x % 60).toFixed(1).padStart(4, '0')}`;
  console.log(`${FILM}: ${info.duration.toFixed(2)} s; voice: ${voice ? NAME : 'none (estimates at 2.5 words/s)'}`);
  const keysOf = Object.fromEntries(script.beats.map(b => [b.key, b.lines.map(l => l.key)]));
  for (const [k, t0] of Object.entries(info.marks)) {
    console.log(`  ${k.padEnd(9)} ${fmt(t0)}`);
    for (const lk of keysOf[k] || []) console.log(`    ${lk.padEnd(10)} ${fmt(info.lines[lk].t)}, ${info.lines[lk].duration.toFixed(2)} s${info.lines[lk].voiced ? '' : ' (estimated)'}`);
  }
  console.log(`  music: entries ${info.music.open.join(', ')} s, closing motif ${info.music.end} s; poster at ${info.poster?.toFixed(2)} s`);
  if (info.missing.length) console.warn(`  not found, placed by estimate: ${info.missing.join('; ')}`);
} else if (option(args, '--preview')) {
  // A stretch of the video, silent, to check motion: --preview FROM,TO
  const [from, to] = option(args, '--preview').split(',').map(Number);
  const pages = [];
  for (let i = 0; i < WORKERS; i++) pages.push(await open(option(args, '--cut', 'video')));
  const file = path.join(out, 'preview.mp4');
  await encodeParallel({ pages, fps: FPS, from, to, label: 'preview', sinks: [{ outArgs: videoArgs(file, null) }] });
  report(file, root);
} else if (args.includes('--poster-only')) {
  const info = await open('video');
  const poster = path.join(media, `${FILM}-poster.png`);
  await info.page.evaluate(() => window.__captions(false));
  fs.writeFileSync(poster, await info.frame(+option(args, '--poster', String(info.poster))));
  report(poster, root);
} else if (args.includes('--audio-only')) {
  const mp4 = path.join(media, `${FILM}.mp4`);
  const info = await open('video');
  const track = sound(info);
  if (!track) throw new Error(`${FILM}: nothing to mix`);
  if (fs.existsSync(mp4)) { remux(mp4, track); report(mp4, root); }
  else console.log(`${FILM}: no media/${FILM}.mp4 yet; the mix is in ${path.relative(root, track)}`);
} else {
  if (only === 'all' || only === 'video') {
    const first = await open('video');
    const pages = [first];
    for (let i = 1; i < WORKERS; i++) pages.push(await open('video'));
    console.log(`${FILM}: ${first.duration.toFixed(2)} s, ${WORKERS} page${WORKERS > 1 ? 's' : ''}`);
    const track = sound(first);
    const mp4 = path.join(media, `${FILM}.mp4`);
    const secs = await encodeParallel({ pages, fps: FPS, to: first.duration, label: FILM, sinks: [{ outArgs: videoArgs(mp4, track), audio: track }] });
    report(mp4, root);
    const posterT = +option(args, '--poster', String(first.poster ?? first.marks.idea + 12));
    const poster = path.join(media, `${FILM}-poster.png`);
    await first.page.evaluate(() => window.__captions(false));
    fs.writeFileSync(poster, await first.frame(posterT));
    report(poster, root);
    console.log(`${FILM}: rendered in ${(secs / 60).toFixed(1)} min`);
    await closeAll();
  }
  if (only === 'all' || only === 'teaser') {
    // Only a film whose film.js has a teaser cut (the explainer's) makes one.
    const first = await open('teaser');
    if (!first.hasTeaser) {
      if (only === 'teaser') console.warn(`${FILM}: no teaser cut in film.js`);
      await closeAll();
      process.exit(0);
    }
    const pages = [first];
    for (let i = 1; i < WORKERS; i++) pages.push(await open('teaser'));
    const gif = path.join(media, `${FILM}-teaser.gif`);
    const webm = path.join(media, `${FILM}-teaser.webm`);
    await encodeParallel({
      pages, fps: FPS, to: first.duration, label: 'teaser', sinks: [
        // 64 colours, undithered: the flat palette keeps the GIF near 1 MB.
        { outArgs: ['-filter_complex', 'fps=15,scale=1280:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=64:stats_mode=full[p];[b][p]paletteuse=dither=none:diff_mode=rectangle', '-loop', '0', gif] },
        { outArgs: ['-vf', 'scale=1280:-2:flags=lanczos,format=yuv420p', '-c:v', 'libvpx-vp9', '-b:v', '0', '-crf', '32', '-row-mt', '1', '-deadline', 'good', '-an', webm] },
      ],
    });
    report(gif, root);
    report(webm, root);
    await closeAll();
  }
}
await closeAll();
