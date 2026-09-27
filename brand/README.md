# stretto brand kit

Everything stretto shows the world, and the sources to make it again: the logo, colors and type, the words, a social card, a visual explainer, a launch video and a terminal walkthrough. The tone is calm and exact: stretto makes the reads an agent would make next, and says how much that saves only with the model, the benchmark and the kind of evidence next to the number.

| What | Where | Made by |
|---|---|---|
| Mark, wordmark, lockups | [`logo/`](logo/) (SVG) | `tools/build_logo.py` |
| Favicon and app icons | `logo/favicon.svg`, `favicon.ico` (16, 32, 48), `apple-touch-icon.png` (180), `icon-512.png` | `tools/build_logo.py`, `tools/render_assets.mjs` |
| Color tokens | [`tokens.css`](tokens.css), documented in [`palette.md`](palette.md) | by hand; checked by `tools/contrast.mjs` |
| Fonts | [`fonts/`](fonts/): Inter and JetBrains Mono, subset, with their licenses | `tools/subset_fonts.py` |
| Words | [`messaging.md`](messaging.md): taglines, descriptions, key messages, terms | by hand, from `docs/results/claims.md` |
| Social card | `social/og-card.png` (1200 × 630), from `social/og-card.html` | `tools/render_assets.mjs` |
| Visual explainer | [`explainer/index.html`](explainer/index.html), one self-contained file | by hand; fonts by `tools/embed_fonts.py` |
| Launch video | `media/launch.mp4`, `launch-poster.png`, `launch-teaser.gif`, `launch-teaser.webm` | `video/launch/render.mjs` |
| Walkthrough video | `media/walkthrough.mp4`, `walkthrough-poster.png` | `video/walkthrough/` (capture, then render) |

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
<meta property="og:image" content="https://alexnodeland.github.io/stretto/og-card.png">
<meta property="og:image:width" content="1200">
<meta property="og:image:height" content="630">
<meta property="og:image:alt" content="stretto: Read ahead of your agent. Live, GLM-5.3 took 27.9% fewer LLM turns on 28 τ²-bench retail and airline tasks.">
<meta name="twitter:card" content="summary_large_image">
```

## Explainer

`explainer/index.html` explains stretto in seven steps: where the proxy sits, recording sessions, learning a flow, serving it, what a detour costs, the decision rule, and the results with their scope. It is one file with its styles, script, drawings and fonts inline (177 KB), so it can be served from anywhere, and it links out only to the paper, the claims ledger and the walkthrough on GitHub.

- **Controls:** play and pause, previous and next, a progress bar whose segments jump to a step; the arrow keys, Home and End; space or K to play and pause; swipes on touch screens. It starts playing once it is in view and stops after the last step.
- **Accessibility:** it follows `prefers-reduced-motion` (no animation; Play still steps through), announces each step to screen readers when not playing, keeps inactive steps out of the tab order, and gives the results chart a data table.
- **Themes:** light and dark follow `prefers-color-scheme`; `?theme=light|dark` or a message forces one. It fits a 390 px screen with no horizontal scroll.
- **Deep links:** `?step=4` opens on a step without playing; `?autoplay=0` waits for Play.

Embed it at `/stretto/explainer/`:

```html
<iframe src="/stretto/explainer/" title="How stretto works" loading="lazy"
        style="width: 100%; height: 760px; border: 0;"></iframe>
<script>
  // Optional: size the frame to its content, and pass the site's theme.
  const frame = document.querySelector('iframe[title="How stretto works"]');
  addEventListener('message', e => {
    if (e.source === frame.contentWindow && e.data?.type === 'stretto-explainer:height') frame.style.height = e.data.height + 'px';
  });
  // frame.contentWindow.postMessage({ type: 'stretto-explainer:theme', theme: 'dark' }, '*');  // or 'light', 'auto'
</script>
```

It is 743 px tall at 1000 px wide and 772 px at 720 px, the same for every step; on a 390 px phone, where only the current step takes space, 785 to 1,069 px.

## Videos

| File | Length | Size | Notes |
|---|---|---|---|
| `media/launch.mp4` | 87.2 s, 1920 × 1080, 30 fps | 4.5 MB | H.264 High, yuv420p, BT.709, faststart, no audio |
| `media/launch-poster.png` | 1920 × 1080 | 73 KB | The lockup and tagline, at 3.9 s |
| `media/launch-teaser.gif` | 8 s loop, 1280 × 720, 15 fps | 0.43 MB | For the README |
| `media/launch-teaser.webm` | 8 s loop, 1280 × 720 | 0.11 MB | For the site (VP9) |
| `media/walkthrough.mp4` | 159.5 s, 1920 × 1080, 30 fps | 8.7 MB | The same encoding as the launch video |
| `media/walkthrough-poster.png` | 1920 × 1080 | 172 KB | The served session: the two reads riding in the search's result |

**The launch video** has no voice-over; titles and captions carry it: the problem (an LLM turn per call, many of them fixed by what the tools returned), the idea, how it works (record, learn, serve, the decision rule), the results with their scope, and how to start. Its source is `video/launch/`: `index.html` lays out a 1920 × 1080 stage and `timeline.js` keyframes every property as a function of time, so a frame at t is always the same frame. Preview it in a browser with `index.html?play`, or one frame with `index.html?t=42`.

**The walkthrough** is a real terminal session: `docs/walkthrough.md`, run with the release binaries on the official MCP filesystem server, with a scripted agent standing in for the LLM, and no key. `video/walkthrough/script.sh` is the command list; `capture.py` runs it in a pseudo-terminal and records every byte of output with its timing to `walkthrough.cast` (asciicast v2, with markers for steps, captions and commands); `render.mjs` draws the cast as a terminal, with a title card per step, and pipes the frames to ffmpeg. The only change to the output is the working directory's path, shown as `/home/me` as in the docs.

```html
<video controls preload="none" poster="/stretto/media/launch-poster.png" width="1920" height="1080" style="width: 100%; height: auto;">
  <source src="/stretto/media/launch.mp4" type="video/mp4">
</video>
```

## Rebuilding

Requirements: Node 18 or later, Python 3.10 or later, and for the walkthrough the stretto binaries and npx (the filesystem server comes from npm). No step needs a key.

```sh
cd brand
npm install                                    # Playwright (1.56.1); `npx playwright install chromium` if it has no browser
python3 -m venv .venv && .venv/bin/pip install -r tools/requirements.txt
export FFMPEG=$(.venv/bin/python -c 'import imageio_ffmpeg as i; print(i.get_ffmpeg_exe())')   # or have ffmpeg on PATH

.venv/bin/python tools/build_logo.py           # logo/*.svg
node tools/render_assets.mjs                   # favicon.ico, the PNG icons, social/og-card.png
node tools/contrast.mjs --check                # after changing tokens.css
.venv/bin/python tools/embed_fonts.py explainer/index.html   # after editing the explainer's text

node video/launch/render.mjs                   # media/launch.mp4, its poster and the teaser (about 4 minutes)

# The walkthrough: record it, then render it (about 6 minutes).
python3 video/walkthrough/capture.py --bin /path/to/stretto/binaries
node video/walkthrough/render.mjs
```

Frames are piped straight into ffmpeg; nothing is written to disk but the outputs. `--stills 12,40.5 --stills-dir DIR` on either renderer writes single frames instead, to check a change. Set `CHROMIUM_PATH` to use a Chromium other than Playwright's.

**When the CLI changes,** edit `video/walkthrough/script.sh` and record again. A step is a `step TITLE CAPTION` line followed by `run COMMAND` lines (`note CAPTION` changes the caption between commands); a new step, such as `stretto init` or `stretto doctor`, is a new block in the order it belongs, and the capture and the renderer need no change. `bash video/walkthrough/script.sh` runs the same steps in a terminal, without recording.

## For the docs site and the README

Copy into `website/public/` (served at `/stretto/`):

| From `brand/` | To `website/public/` |
|---|---|
| `logo/favicon.ico`, `logo/favicon.svg`, `logo/apple-touch-icon.png`, `logo/icon-512.png` | the same names, at the root |
| `social/og-card.png` | `og-card.png` |
| `explainer/index.html` | `explainer/index.html` |
| `logo/stretto-lockup.svg`, `logo/stretto-lockup-dark.svg`, `logo/stretto-mark.svg` | `logo/` (for the header) |
| `media/launch.mp4`, `media/launch-poster.png`, `media/launch-teaser.webm`, `media/walkthrough.mp4`, `media/walkthrough-poster.png` | `media/` |
| `tokens.css`, and `fonts/*` with the two licenses if the site serves its own fonts | where the site keeps its styles and fonts |

In the site's `<head>`:

```html
<link rel="icon" href="/stretto/favicon.ico" sizes="48x48">
<link rel="icon" href="/stretto/favicon.svg" type="image/svg+xml">
<link rel="apple-touch-icon" href="/stretto/apple-touch-icon.png">
<link rel="manifest" href="/stretto/site.webmanifest">
<meta name="theme-color" content="#f8fbfb" media="(prefers-color-scheme: light)">
<meta name="theme-color" content="#0b0f11" media="(prefers-color-scheme: dark)">
```

with `site.webmanifest`: `{"name": "stretto", "short_name": "stretto", "icons": [{"src": "/stretto/icon-512.png", "sizes": "512x512", "type": "image/png", "purpose": "any maskable"}], "background_color": "#0b0f11", "theme_color": "#0b0f11", "display": "browser"}`.

In the repository's `README.md`, the lockup (switching with the reader's theme) and the teaser:

```html
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="brand/logo/stretto-lockup-dark.svg">
  <img alt="stretto" src="brand/logo/stretto-lockup.svg" height="44">
</picture>

**Read ahead of your agent.** stretto learns, from your agent's recorded tool calls, which reads it makes next and where their arguments come from, and serves them through an MCP proxy in the same tool result, so the agent needs fewer LLM turns.

![After each call, the reads the agent would make next ride in the same tool result, and it skips those turns](brand/media/launch-teaser.gif)
```

GitHub does not play a video committed to the repository inline; link the MP4s from the README to the docs site, where the `<video>` tag above plays them.

## Licenses

The brand kit's code (tools and render scripts) is MIT, as the repository is. The fonts in `fonts/`, and the fonts embedded in `explainer/index.html`, are under the SIL Open Font License 1.1: Inter, copyright 2016 The Inter Project Authors; JetBrains Mono, copyright 2020 The JetBrains Mono Project Authors.
