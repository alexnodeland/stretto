// Render the walkthrough video from the recorded session.
//
//   node render.mjs                        # brand/media/walkthrough.mp4 and walkthrough-poster.png
//   node render.mjs --cast other.cast      # from another recording (default: walkthrough.cast)
//   node render.mjs --poster-only          # only the poster
//   node render.mjs --audio-only           # the voice-over and captions again, into the walkthrough.mp4 there is
//   node render.mjs --stills 12,40.5 --stills-dir /tmp/stills   # check frames, as PNG
//
// Record the session first with capture.py (see brand/README.md). term.html
// draws each frame from the cast with window.__render(t); frames go straight
// into ffmpeg's stdin. The voice-over is narrate.py's clips: term.js paces the
// video so that each caption's line finishes before the next caption, and says
// when each starts. Its captions go to media/walkthrough.vtt.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { launchBrowser, openPage, encode, h264Args, report, option, loadVoice, voiceTrack, writeVtt, remux } from '../lib.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..', '..', '..');
const media = path.resolve(here, '..', '..', 'media');
const args = process.argv.slice(2);
const FPS = +option(args, '--fps', '30');
const cast = fs.readFileSync(path.resolve(option(args, '--cast', path.join(here, 'walkthrough.cast'))), 'utf8');

const voice = loadVoice('walkthrough');
const line = key => voice.lines.find(l => l.key === key);
const lengths = voice && {
  intro: line('intro').duration,
  outro: line('outro').duration,
  captions: Object.fromEntries(voice.lines.filter(l => l.key.startsWith('caption-')).map(l => [l.text, l.duration])),
};
const browser = await launchBrowser();
const { page, frame } = await openPage(browser, pathToFileURL(path.join(here, 'term.html')).href,
  p => p.evaluate(text => window.__load(text), cast), lengths);
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
  // The voice-over: each line term.js scheduled, matched to its clip (the
  // intro, the outro, and a caption by its text), mixed into one track.
  const voiced = async () => {
    const spoken = await page.evaluate(() => window.__spoken);
    const clips = spoken.map(s => {
      const l = s.key === 'caption' ? voice.lines.find(v => v.key.startsWith('caption-') && v.text === s.text) : line(s.key);
      if (!l) throw new Error(`walkthrough: no voice for ${s.key} ${s.text || ''}: run narrate.py again`);
      return { ...l, t: s.t };
    });
    clips.forEach((c, i) => {
      if (clips[i + 1] && c.t + c.duration > clips[i + 1].t) throw new Error(`walkthrough: ${c.key} runs into ${clips[i + 1].key}`);
    });
    const audio = path.join(here, '..', 'out', 'voice', 'walkthrough', 'track.wav');
    voiceTrack(clips, duration, audio);
    const vtt = path.join(media, 'walkthrough.vtt');
    writeVtt(clips, vtt);
    report(vtt, root);
    return audio;
  };
  if (args.includes('--audio-only')) {
    if (!voice) throw new Error('walkthrough: --audio-only needs the voice-over (narrate.py)');
    remux(out, await voiced());
    report(out, root);
  } else if (!args.includes('--poster-only')) {
    const audio = voice ? await voiced() : null;
    await encode({ frame, fps: FPS, from: 0, to: duration, outArgs: h264Args(out, FPS, 20, !!audio), label: 'walkthrough', audio });
    report(out, root);
  }
  const poster = path.join(media, 'walkthrough-poster.png');
  fs.writeFileSync(poster, await frame(posterT));
  report(poster, root);
}
await browser.close();
