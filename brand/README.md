# stretto brand kit

Everything stretto shows the world, and the sources to make it again: the logo, colors and type, the words, a social card, an interactive explainer, an explainer video and a terminal walkthrough, the last three narrated. The tone is calm and exact: stretto makes the reads an agent would make next, and says how much that saves only with the model, the benchmark and the kind of evidence next to the number.

| What | Where | Made by |
|---|---|---|
| Mark, wordmark, lockups | [`logo/`](logo/) (SVG) | `tools/build_logo.py` |
| Favicon and app icons | `logo/favicon.svg`, `favicon.ico` (16, 32, 48), `apple-touch-icon.png` (180), `icon-512.png` | `tools/build_logo.py`, `tools/render_assets.mjs` |
| Color tokens | [`tokens.css`](tokens.css), documented in [`palette.md`](palette.md) | by hand; checked by `tools/contrast.mjs` |
| Fonts | [`fonts/`](fonts/): Inter and JetBrains Mono, subset, with their licenses | `tools/subset_fonts.py` |
| Words | [`messaging.md`](messaging.md): taglines, descriptions, key messages, terms | by hand, from `docs/results/claims.md` |
| Social card | `social/og-card.png` (1200 × 630), from `social/og-card.html` | `tools/render_assets.mjs` |
| Interactive explainer | [`explainer/index.html`](explainer/index.html), one self-contained file; its narration in `explainer/audio/` | by hand; fonts by `tools/embed_fonts.py` |
| Explainer video | `media/explainer.mp4`, `explainer-poster.png`, `explainer.vtt`, `explainer-teaser.gif`, `explainer-teaser.webm` | [`video/explainer/`](video/explainer/README.md), with `video/music.py` |
| Walkthrough video | `media/walkthrough.mp4`, `walkthrough-poster.png`, `walkthrough.vtt` | `video/walkthrough/` (capture, then render) |
| The math film | `media/math.mp4`, `math-poster.png`, `math.vtt` | [`video/math/`](video/math/README.md) |
| The console film | `media/console.mp4`, `console-poster.png`, `console.vtt` | [`video/console/`](video/console/README.md) (capture, then render) |
| Console screenshots | `media/console/`: the overview, a server, a session, a flow and a job, each `-light.png` and `-dark.png` (1440 × 900, 256 colors) | Playwright, from the real console on its test fixtures |
| Voice-over | the lines in `video/explainer/narration.json`, `video/walkthrough/narration.json` (with the walkthrough's captions) and `explainer/narration.json` | `video/narrate.py`, with Kokoro |

## Logo

**The mark** is a stretto of three entries. In a fugue's stretto, each entry of the subject comes in before the last one has finished; here the same bar enters three times, each before the last ends and each sooner than the one before. The first bar is ink, the agent's own call; the two that overlap it are petrol, the reads stretto makes ahead of it.

**The wordmark** is "stretto", always lowercase, in Inter Display SemiBold converted to outlines, so it needs no font. Inter's kerning joins the crossbars of the two t's into one: two entries, overlapping.

| File | Use |
|---|---|
| `stretto-lockup.svg` | The default: mark and wordmark, on light backgrounds |
| `stretto-lockup-dark.svg` | The same on dark backgrounds (paper and a lighter petrol) |
| `stretto-mark.svg`, `stretto-mark-dark.svg` | The mark alone, where the name is already on screen (a sidebar, an avatar) |
| `stretto-wordmark.svg`, `stretto-wordmark-dark.svg` | The name alone, where the mark would crowd |
| `favicon.svg`, `favicon.ico` | Browser tabs. A petrol tile with paper bars, so it reads on light and dark tab bars; its bars fall on whole pixels at 16 and 32 px |
| `app-icon.svg`, `apple-touch-icon.png`, `icon-512.png` | Home screens and web-app manifests. Full-bleed squares; the bars sit inside the maskable safe zone, so `"purpose": "any maskable"` is safe |

- **Clear space:** keep at least one bar's height free on every side of the mark or the lockup.
- **Size:** the lockup reads down to 20 px tall; the mark alone down to 16 px. Below 24 px on a background you do not control, use the favicon tile.
- **Color:** ink and petrol on light, paper and light petrol on dark, as the files come. Do not recolor, outline, rotate, skew, add effects, or reset the name in another typeface or with a capital S.

## Color

One accent, petrol, and neutrals leaning toward its cool hue. Petrol marks what stretto adds: its reads in a diagram, a link, the primary action. The rest is ink on paper, or paper on ink. [`palette.md`](palette.md) lists every token with its use, and the contrast of all 80 foreground and background pairs: text at 4.5:1 or more and marks at 3:1 or more, in both themes (WCAG 2.2 AA).

```css
@import url("tokens.css");
body { background: var(--stretto-bg); color: var(--stretto-text); font-family: var(--stretto-font-sans); }
a { color: var(--stretto-accent); }
```

`tokens.css` switches to dark under `prefers-color-scheme: dark`, and `<html data-theme="light|dark">` forces either. After changing a token, run `node brand/tools/contrast.mjs --check`.

## Type

- **Inter** (4.001, variable weight and optical size) for everything but code, by Rasmus Andersson and the Inter Project Authors.
- **JetBrains Mono** (2.242) for code and terminals, by JetBrains.

Both are under the [SIL Open Font License 1.1](fonts/LICENSE-Inter.txt) ([JetBrains Mono's](fonts/LICENSE-JetBrainsMono.txt)), which allows use, subsetting and redistribution with the license; neither reserves a font name. `fonts/` holds subsets of both (Latin, Greek, arrows, math symbols, box drawing for the terminal), taken from the npm packages `inter-ui@4.1.1` and `jetbrains-mono@1.0.6`. Every page and video here loads these files or embeds them, so renders do not depend on installed fonts. `tokens.css` names both with system fallbacks.

## Words

[`messaging.md`](messaging.md) has the tagline (recommended: **Read ahead of your agent.**), a one-liner, 50- and 150-word descriptions, six key messages each tied to a row of [the claims ledger](../docs/results/claims.md), the terms to use, and the words to avoid. The rule for every number: take it from the ledger or the paper, and give its scope (which model, which benchmark, live or replayed).

## Social card

`social/og-card.png` is 1200 × 630: the lockup, the tagline, a schematic of the turns a flow saves, and the live result with its scope. It is rendered from `social/og-card.html` (`node brand/tools/render_assets.mjs og`).

```html
<meta property="og:image" content="https://stretto.alexnodeland.com/og.png">
<meta property="og:image:width" content="1200">
<meta property="og:image:height" content="630">
<meta property="og:image:alt" content="stretto: Read ahead of your agent. Live on 28 τ²-bench retail and airline tasks, Claude Sonnet 5, Claude Haiku 4.5 and GLM-5.3 took 20–28% fewer LLM turns.">
<meta name="twitter:card" content="summary_large_image">
```

## Interactive explainer

`explainer/index.html` explains stretto in seven steps: where the proxy sits, recording sessions, learning a flow, serving it, what a detour costs, the decision rule, and the results with their scope. It is one file with its styles, script, drawings and fonts inline (180 KB), so it can be served from anywhere, and it links out only to the live results, the claims ledger and the walkthrough on GitHub.

- **Controls:** play and pause, previous and next, a progress bar whose segments jump to a step; the arrow keys, Home and End; space or K to play and pause; swipes on touch screens. It starts playing once it is in view and stops after the last step.
- **Narration:** the speaker button, or M, reads each step aloud: `audio/step-N.mp3` beside the page (0.9 MB in all, fetched only once the sound is on). It is off at first, since browsers play sound only after a click. With it on, a playing explainer moves on when a step's line has finished. Served without `audio/`, the button hides itself.
- **Accessibility:** it follows `prefers-reduced-motion` (no animation; Play still steps through), announces each step to screen readers when not playing, keeps inactive steps out of the tab order, and gives the results chart a data table.
- **Themes:** light and dark follow `prefers-color-scheme`; `?theme=light|dark` or a message forces one. It fits a 390 px screen with no horizontal scroll.
- **Deep links:** `?step=4` opens on a step without playing; `?autoplay=0` waits for Play.

Embed it at `/explainer/`:

```html
<iframe src="/explainer/" title="How stretto works" loading="lazy"
        style="width: 100%; height: 780px; border: 0;"></iframe>
<script>
  // Optional: size the frame to its content, and pass the site's theme.
  const frame = document.querySelector('iframe[title="How stretto works"]');
  addEventListener('message', e => {
    if (e.source === frame.contentWindow && e.data?.type === 'stretto-explainer:height') frame.style.height = e.data.height + 'px';
  });
  // frame.contentWindow.postMessage({ type: 'stretto-explainer:theme', theme: 'dark' }, '*');  // or 'light', 'auto'
</script>
```

It is 760 px tall at 1000 px wide and 876 px at 720 px, the same for every step; on a 390 px phone, where only the current step takes space, 785 to 1,174 px.

## Videos

| File | Length | Size | Notes |
|---|---|---|---|
| `media/explainer.mp4` | 200.5 s, 1920 × 1080, 30 fps | 10.9 MB | H.264 High, yuv420p, BT.709, faststart; voice and music in AAC, at -16 LUFS |
| `media/explainer.vtt` | 45 cues | 4 KB | The voice-over's captions (WebVTT), each starting on its first spoken word |
| `media/explainer-poster.png` | 1920 × 1080 | 0.5 MB | One tool result carrying the two reads stretto made, and the turns closed up from five to three |
| `media/explainer-teaser.gif` | 8 s loop, 1280 × 720, 15 fps | 0.6 MB | For the README |
| `media/explainer-teaser.webm` | 8 s loop, 1280 × 720 | 0.1 MB | For the site (VP9) |
| `media/walkthrough.mp4` | 192.6 s, 1920 × 1080, 30 fps | 10.5 MB | H.264 High, faststart; the voice-over in AAC, at -16.5 LUFS |
| `media/walkthrough.vtt` | 37 cues | 3 KB | The voice-over's captions (WebVTT) |
| `media/walkthrough-poster.png` | 1920 × 1080 | 201 KB | The served session: the two reads riding in the search's result |
| `media/math.mp4` | 208.9 s, 1920 × 1080, 30 fps | 9.6 MB | H.264 High, faststart; voice and music in AAC, at -16 LUFS |
| `media/math.vtt`, `media/math-poster.png` | 1920 × 1080 | | Captions; the poster is alpha's posterior, the chain's draws piled into it |
| `media/console.mp4` | 122.3 s, 1920 × 1080, 30 fps | 8.5 MB | H.264 High, faststart; voice and music in AAC, at -16 LUFS |
| `media/console.vtt`, `media/console-poster.png` | 1920 × 1080 | | Captions; the poster is a served session's three lookups |

**The explainer video** tells the story in ten chapters between the title and an end card, one diagram each, on a still stage: an agent's turns and what they cost; the turns the tools had already decided; stretto making those reads, inside the result the agent asked for; why a flow only reads; how a flow is learned and reviewed; the console; the rule that decides a lookup; the live results; the prompt baseline; and how to start. What it shows is the real thing: the proxy's wording for the reads it adds to a result, τ²-bench retail's tools, the counts of a flow in `docs/examples/`, the console's own screens and the commands of the docs. The line being spoken is set at the foot of the frame, each word lit as it is said, so the film reads with the sound off. Its pacing follows Auracle's films ([`video/PACING.md`](video/PACING.md)), and [`video/explainer/README.md`](video/explainer/README.md) lists the chapters and how the timing follows the voice.

**The deep dives** are drawn on the explainer's stage (`video/stage/`), at its pace and in its voice, and rendered by the same `video/film.mjs`. [The math film](video/math/README.md) is for engineers: the event a lookup is decided on, why a flow only reads, the rule and its costs, the counts, alpha's posterior as a fugue program, the bindings, and how the estimate held up in replay and live, with the code that computes them. Its formulas are typeset with KaTeX (`npm install` in `brand/`), its derivations worked in place, and its charts drawn from the published rows and from fugue's own run on the console's fixtures (`video/math/data/`). [The console film](video/console/README.md) tours `stretto-console`, from shots of the real console over its test fixtures, captured by driving it as a user does.

**The walkthrough** is a real terminal session, with the release binaries on the official MCP filesystem server, a scripted agent standing in for the LLM, and no key: `stretto doctor`, then `stretto init` printing Claude Code's configuration, then the loop `docs/walkthrough.md` runs (record, learn, review, audit, serve, learn again). `video/walkthrough/script.sh` is the command list; `capture.py` runs it in a pseudo-terminal, with a small allowlisted environment and the working directory as `HOME` (the binaries copied into its `.cargo/bin`, where `cargo install` puts them), and records every byte of output with its timing to `walkthrough.cast` (asciicast v2, with markers for steps, captions and commands); `render.mjs` draws the cast as a terminal, with a title card per step, and pipes the frames to ffmpeg. The only change to the output is the working directory's path, shown as `/home/me` as in the docs. The voice says the intro, each caption as it appears (`video/walkthrough/narration.json` gives a spoken form where the text on screen has paths or flags), and the outro; `term.js` holds each caption until its line has finished.

```html
<video controls preload="none" poster="/media/explainer-poster.png" width="1920" height="1080" style="width: 100%; height: auto;">
  <source src="/media/explainer.mp4" type="video/mp4">
  <track kind="captions" src="/media/explainer.vtt" srclang="en" label="English">
</video>
```

**The voice** is Kokoro-82M (hexgrad, Apache-2.0), an open text-to-speech model run locally, in its `af_heart` voice at speed 0.85: the voice of Auracle's films, one voice at one pace, every line at one loudness, for both videos and the interactive explainer. `video/narrate.py` speaks each line whole, so its commas keep their prosody, trims it and sets its level. Then it transcribes the line back with faster-whisper and speaks it again, a touch slower or faster, if a word went missing; the manifest keeps the time of every word, which the explainer video times its beats and captions by. The text is written for reading, and `SAY`, with each script's `say` fields, turns versions, acronyms and numbers into what the voice should say (`GLM-5.3`, "G L M five point three"). "stretto" is said the American way, STRED-oh, with the t flapped; `LEXICON` in `narrate.py` pins it.

**The music** is synthesized, not sampled. `video/music.py` writes a bed in D major whose six-note motif enters three times, each entry before the last has finished, a stretto, landing with the mark's three bars, and returns once to close; the explainer's renderer runs it with the timeline's own times. There are no sound effects: the voice, the bed under it, and the motif in and out.

## Rebuilding

Requirements: Node 18 or later, Python 3.10 or later, and for the walkthrough the stretto binaries, npx (the filesystem server comes from npm) and jq. For the voice-over, Python 3.10 to 3.12, `video/requirements-narrate.txt` (torch's CPU build, from PyTorch's index) and about 1 GB for the models it fetches on first use. No step needs a key.

```sh
cd brand
npm install                                    # Playwright (1.56.1); `npx playwright install chromium` if it has no browser
python3 -m venv .venv && .venv/bin/pip install -r tools/requirements.txt
export FFMPEG=$(.venv/bin/python -c 'import imageio_ffmpeg as i; print(i.get_ffmpeg_exe())')   # or have ffmpeg on PATH

.venv/bin/python tools/build_logo.py           # logo/*.svg
node tools/render_assets.mjs                   # favicon.ico, the PNG icons, social/og-card.png
node tools/contrast.mjs --check                # after changing tokens.css
.venv/bin/python tools/embed_fonts.py explainer/index.html   # after editing the explainer's text

# The voice-over: every line of both videos and the interactive explainer's audio/
# (a few minutes on 4 CPUs; a line whose words and settings have not changed is kept).
python3.12 -m venv .venv-voice
.venv-voice/bin/pip install -r video/requirements-narrate.txt
.venv-voice/bin/python video/narrate.py

# The films on the shared stage: each render makes its music too.
PYTHON=.venv-voice/bin/python node video/film.mjs explainer
PYTHON=.venv-voice/bin/python node video/film.mjs math
sh video/console/capture.sh        # the console's shots, from the real console (see video/console/README.md)
PYTHON=.venv-voice/bin/python node video/film.mjs console

# The walkthrough: record it, then render it (about 7 minutes).
python3 video/walkthrough/capture.py --bin /path/to/stretto/binaries
node video/walkthrough/render.mjs
```

Without the voice-over's clips (`video/out/voice/`, made by `narrate.py`), the renderers make silent videos at the scenes' own lengths. After a change to the words alone, `narrate.py` then `render.mjs --audio-only` puts the new voice-over into the video there is, as long as no line changed length: a line of another length changes the pacing, and needs a full render.

Frames are piped straight into ffmpeg; nothing is written to disk but the outputs. `--stills 12,40.5` on either renderer writes single frames instead, to check a change (`--stills-dir DIR` for the walkthrough's). Set `CHROMIUM_PATH` to use a Chromium other than Playwright's.

**When the CLI changes,** edit `video/walkthrough/script.sh` and record again. A step is a `step TITLE CAPTION` line followed by `run COMMAND` lines (`note CAPTION` changes the caption between commands); a new step is a new block in the order it belongs, and the capture and the renderer need no change. `bash video/walkthrough/script.sh` runs the same steps in a terminal, without recording, in a temporary directory that it makes `HOME`.

## For the docs site and the README

Copy into `website/public/` (served at the site's root):

| From `brand/` | To `website/public/` |
|---|---|
| `logo/favicon.ico`, `logo/favicon.svg`, `logo/apple-touch-icon.png`, `logo/icon-512.png` | the same names, at the root |
| `social/og-card.png` | `og.png` |
| `explainer/index.html` | `explainer/index.html` |
| `logo/stretto-lockup.svg`, `logo/stretto-lockup-dark.svg`, `logo/stretto-mark.svg` | `logo/` (for the header) |
| `explainer/audio/step-1.mp3` … `step-7.mp3` | `explainer/audio/` |
| `media/explainer.mp4`, `media/explainer-poster.png`, `media/explainer.vtt`, `media/explainer-teaser.webm`, `media/walkthrough.mp4`, `media/walkthrough-poster.png`, `media/walkthrough.vtt` | `media/` |
| `tokens.css`, and `fonts/*` with the two licenses if the site serves its own fonts | where the site keeps its styles and fonts |

In the site's `<head>`:

```html
<link rel="icon" href="/favicon.ico" sizes="48x48">
<link rel="icon" href="/favicon.svg" type="image/svg+xml">
<link rel="apple-touch-icon" href="/apple-touch-icon.png">
<link rel="manifest" href="/site.webmanifest">
<meta name="theme-color" content="#f8fbfb" media="(prefers-color-scheme: light)">
<meta name="theme-color" content="#0b0f11" media="(prefers-color-scheme: dark)">
```

with `site.webmanifest`: `{"name": "stretto", "short_name": "stretto", "icons": [{"src": "icon-512.png", "sizes": "512x512", "type": "image/png", "purpose": "any maskable"}], "background_color": "#0b0f11", "theme_color": "#0b0f11", "display": "browser"}`.

In the repository's `README.md`, the lockup (switching with the reader's theme) and the teaser:

```html
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="brand/logo/stretto-lockup-dark.svg">
  <img alt="stretto" src="brand/logo/stretto-lockup.svg" height="44">
</picture>

**Read ahead of your agent.** stretto learns, from your agent's recorded tool calls, which reads it makes next and where their arguments come from, and serves them through an MCP proxy in the same tool result, so the agent needs fewer LLM turns.

![After each call, the reads the agent would make next ride in the same tool result, and it skips those turns](brand/media/explainer-teaser.gif)
```

GitHub does not play a video committed to the repository inline; link the MP4s from the README to the docs site, where the `<video>` tag above plays them.

## Licenses

The brand kit's code (tools and render scripts) is MIT, as the repository is, and so is the music it synthesizes. The voice-over is spoken by Kokoro-82M (hexgrad, Apache-2.0) in its `af_heart` voice, with misaki (hexgrad, Apache-2.0) as its grapheme-to-phoneme front end, and checked with faster-whisper (SYSTRAN, MIT) running Whisper small.en (OpenAI, MIT). The fonts in `fonts/`, and the fonts embedded in `explainer/index.html`, are under the SIL Open Font License 1.1: Inter, copyright 2016 The Inter Project Authors; JetBrains Mono, copyright 2020 The JetBrains Mono Project Authors.
