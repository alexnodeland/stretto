// Render the launch video, its poster and the README teaser from index.html.
//
//   node render.mjs                 # everything, into brand/media/
//   node render.mjs --only launch   # launch.mp4 and launch-poster.png
//   node render.mjs --only teaser   # launch-teaser.gif and launch-teaser.webm
//   node render.mjs --audio-only    # the voice-over and captions again, into the launch.mp4 there is
//   node render.mjs --stills 3,20.5 [--cut teaser] --stills-dir /tmp/stills   # check frames, as PNG
//
// Each frame is drawn by window.__render(t) at t = frame / fps (timeline.js),
// screenshotted by headless Chromium and piped into ffmpeg's stdin (../lib.mjs):
// no frames touch the disk. The voice-over is narrate.py's clips, each placed
// at its scene's start plus its `at` (narration.json); the timeline holds each
// scene until its line has finished. Its captions go to media/launch.vtt.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { launchBrowser, openPage, encode, h264Args, report, option, loadVoice, voiceTrack, writeVtt, remux } from '../lib.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..', '..', '..');
const media = path.resolve(here, '..', '..', 'media');
const args = process.argv.slice(2);
const only = option(args, '--only', 'all');
const FPS = +option(args, '--fps', '30');
const POSTER_T = +option(args, '--poster', '3.9');

const voice = loadVoice('launch');
const browser = await launchBrowser();
const open = async cut => {
  const lines = cut === 'launch' && voice ? Object.fromEntries(voice.lines.map(l => [l.key, { at: l.at, duration: l.duration }])) : null;
  const opened = await openPage(browser, `${pathToFileURL(path.join(here, 'index.html')).href}?cut=${cut}`, undefined, lines && { lines });
  opened.duration = await opened.page.evaluate(() => window.__duration);
  opened.marks = await opened.page.evaluate(() => window.__marks);
  return opened;
};

// Each line at its scene's start plus its `at`, checked to end before the next begins.
function placed(marks) {
  const clips = voice.lines.map(l => {
    if (!(l.key in marks)) throw new Error(`launch: narration line ${l.key} names no scene in timeline.js`);
    return { ...l, t: marks[l.key] + l.at };
  }).sort((x, y) => x.t - y.t);
  clips.forEach((c, i) => {
    const next = clips[i + 1];
    if (next && c.t + c.duration > next.t) throw new Error(`launch: line ${c.key} runs into ${next.key}`);
  });
  return clips;
}
fs.mkdirSync(media, { recursive: true });

const stills = option(args, '--stills');
if (stills) {
  const dir = option(args, '--stills-dir', path.join(here, 'out'));
  const cut = option(args, '--cut', 'launch');
  fs.mkdirSync(dir, { recursive: true });
  const { page, frame } = await open(cut);
  for (const t of stills.split(',').map(Number)) {
    const file = path.join(dir, `${cut}-${t.toFixed(2)}.png`);
    fs.writeFileSync(file, await frame(t));
    console.log(file);
  }
  await page.close();
} else {
  if (args.includes('--audio-only')) {
    if (!voice) throw new Error('launch: --audio-only needs the voice-over (narrate.py)');
    const { page, duration, marks } = await open('launch');
    const clips = placed(marks);
    const audio = path.join(here, '..', 'out', 'voice', 'launch', 'track.wav');
    voiceTrack(clips, duration, audio);
    writeVtt(clips, path.join(media, 'launch.vtt'));
    remux(path.join(media, 'launch.mp4'), audio);
    report(path.join(media, 'launch.mp4'), root);
    await page.close();
  } else if (only === 'all' || only === 'launch') {
    const { page, frame, duration, marks } = await open('launch');
    const out = path.join(media, 'launch.mp4');
    let audio = null;
    if (voice) {
      const clips = placed(marks);
      audio = path.join(here, '..', 'out', 'voice', 'launch', 'track.wav');
      voiceTrack(clips, duration, audio);
      const vtt = path.join(media, 'launch.vtt');
      writeVtt(clips, vtt);
      report(vtt, root);
    }
    console.log(`launch: ${duration.toFixed(1)} s`);
    await encode({ frame, fps: FPS, to: duration, outArgs: h264Args(out, FPS, 18, !!audio), label: 'launch', audio });
    report(out, root);
    const poster = path.join(media, 'launch-poster.png');
    fs.writeFileSync(poster, await frame(POSTER_T));
    report(poster, root);
    await page.close();
  }
  if (!args.includes('--audio-only') && (only === 'all' || only === 'teaser')) {
    const { page, frame, duration } = await open('teaser');
    const gif = path.join(media, 'launch-teaser.gif');
    await encode({
      frame, fps: FPS, to: duration, label: 'teaser (gif)', outArgs: [
        '-filter_complex', 'fps=15,scale=1280:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=96:stats_mode=full[p];[b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle',
        '-loop', '0', gif,
      ],
    });
    report(gif, root);
    const webm = path.join(media, 'launch-teaser.webm');
    await encode({
      frame, fps: FPS, to: duration, label: 'teaser (webm)', outArgs: [
        '-vf', 'scale=1280:-2:flags=lanczos,format=yuv420p', '-c:v', 'libvpx-vp9', '-b:v', '0', '-crf', '34',
        '-row-mt', '1', '-deadline', 'good', '-an', webm,
      ],
    });
    report(webm, root);
    await page.close();
  }
}
await browser.close();
