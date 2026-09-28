# The explainer video

A narrated explainer of stretto, 2 minutes 46 seconds long with the current voice-over, drawn as motion graphics: one illustrated world in SVG, and a virtual camera that pans, zooms, rolls and tilts across it. It renders to `brand/media/`:

| File | What it is |
|---|---|
| `explainer.mp4` | 1920 × 1080, 30 fps, H.264 High with faststart (`h264Args`, tuned below) and AAC sound: voice, music bed and sound effects; 16.9 MB with the current voice-over (it must stay under 20 MB) |
| `explainer.vtt` | Captions: each line's `text` from `narration.json`, timed by the voice's own words |
| `explainer-poster.png` | The frame where stretto holds a result with its two reads clipped under it (`window.__poster`), 1.2 MB |
| `explainer-teaser.gif`, `.webm` | An 8-second loop at 1280 × 720 (the GIF at 15 fps, 1.15 MB; the WebM 0.33 MB): a call through stretto, two reads riding back in its result, the turns closing up |

## Rendering

One command makes everything from the voice-over's manifest (`video/out/voice/explainer-video/manifest.json`): the timeline and its anchors, the music's cues, the sound effects, the captions, the poster and the teaser. After a new voice-over, run it again; nothing needs editing by hand.

```sh
cd brand
PLAYWRIGHT_BROWSERS_PATH=/path/to/playwright-browsers \
FFMPEG=/path/to/ffmpeg \
PYTHON=/path/to/python-with-numpy-and-soundfile \
node video/explainer/render.mjs
```

`PLAYWRIGHT_BROWSERS_PATH` is needed only if Playwright's Chromium is not in its default place; `FFMPEG` only if ffmpeg is not on PATH (else imageio-ffmpeg's is used); `PYTHON` is the Python that runs `video/music.py` (it needs numpy and soundfile; without it the mix has no music, and the renderer says so). On 4 CPUs it takes about 10 minutes: 8 to 9.5 for the video's 4,977 frames with the default two pages, under half a minute for the teaser, and the rest for the music and the mix.

```sh
node video/explainer/render.mjs --info                  # the timeline the voice sets, without rendering: scene starts, cues, anchors not found
node video/explainer/render.mjs --only video            # explainer.mp4, explainer.vtt, explainer-poster.png
node video/explainer/render.mjs --only teaser           # explainer-teaser.gif and explainer-teaser.webm
node video/explainer/render.mjs --audio-only            # the sound and captions again, into the explainer.mp4 there is
node video/explainer/render.mjs --stills 12,48.5 --width 960   # single frames, to video/out/stills/
node video/explainer/render.mjs --preview 88,95         # a silent stretch, to video/out/preview.mp4
```

Options: `--workers N` (Chromium pages drawing frames at once; default CPUs − 1, at most 2), `--crf N` (default 26), `--fps`, `--poster T`, `--cut teaser` (for `--stills` and `--preview`). The mix is written to `video/out/explainer/mix.wav` and the music to `video/out/music/explainer-video.wav`; nothing is written into `video/out/voice/`.

The video goes into git and onto GitHub Pages, so it is encoded to stay under 20 MB: `h264Args` (High, BT.709, yuv420p, tune animation, faststart) at CRF 26 (`--crf`), preset veryslow, adaptive quantization mode 3 (it spends bits on dark, flat areas), and an error-diffused conversion from the page's RGB to limited-range YUV, so the dark gradients (the vignette, the halos, the fog) do not band. Checked against the page's own frames with their darks lifted threefold, the encode shows the faint steps the page's 8-bit gradients already have, and at normal contrast none.

Every frame is drawn by `window.__render(t)`, screenshotted by headless Chromium and piped into ffmpeg's stdin (`../lib.mjs`). No frame is written to disk. Several pages draw frames at once, and ffmpeg receives them in order. A frame is a pure function of `t`: no CSS transitions or animations, no timers, and randomness only from seeded generators (`E.rng`), so the pages can draw frames out of order and the result is the same. To preview in a browser, serve `brand/` over HTTP and open `video/explainer/index.html?play`, or `?t=48.5` for one frame; the page then reads `narration.json` itself and estimates the voice's timing.

## The files

| File | What it holds |
|---|---|
| `index.html`, `style.css` | The stage: the parallax grid, the dust, the world (`#world`, one SVG in world units), the vignette and the overlay (HUD, iris, wipes, titles) |
| `engine.js` | Easing (cubic, quint, back, springs), a seeded RNG, keyframe tracks, nodes (SVG groups whose transform and opacity are set each frame), and the camera's paths: van Wijk and Nuij's smooth zoom-and-pan, and a dive that keeps its target on a straight screen path |
| `art.js` | The illustrations: the agent (an orb whose core and orbits think), the customer, the conversation, the MCP server (read drawers; write controls behind a lock), the stretto lens (the mark's three bars in glass), cards, lookups, turn chips, the HUD, the mark and wordmark |
| `world.js` | Where each station sits in the world, and the tableau |
| `scenes.js` | Shared machinery, then the open, the turns, the decided turns, the idea and what stretto cannot touch |
| `scenes2.js` | Learning a flow, the rule, the results, the prompt baseline, getting started, the end card, the teaser, and the order of the scenes |
| `main.js` | Timing from the voice, the camera, drift, motion blur, the dust, and `window.__render` |
| `render.mjs` | The renderer: frames, the sound mix, captions, poster, teaser |
| `narration.json` | The script: each line's `text` (captions), `say` (what the voice says) and `visual` (the storyboard) |

## The scenes

Times are from the render with the current voice-over (the video is 2:45.9 long); they move with it.

| # | Scene (line) | Starts | What happens | Camera and transition in |
|---|---|---|---|---|
| 1 | `open` | 0:00 | On black, a faint staff draws on; the mark's three bars enter along it, each before the last has finished (a stretto), with the music's three entries; the wordmark writes on letter by letter; the tagline | A slow push-in |
| 2 | `turns` | 0:06 | The bars fold into a point of light; an iris opens from it onto the agent's core. The customer asks; the agent reads the whole conversation (a beam scans it), weighs "answer the user" against "call a tool", and calls `get_user_details`; the result comes back. Then the turns repeat faster, 14 of them, while the HUD counts turns, time and tokens | **Iris**, then a pull back from the core; the camera follows the first call, then pulls back as the timeline fills |
| 3 | `decided` | 0:24 | Freeze: the first result lifts out of the conversation. Inside it, the user record: the two order ids light on "two orders", and lines run from them to turns 2 and 3, ghosted; "decided by the tools"; the customer's request dims; the model types each call out | **Zoom-through**: the camera dives into the card until it is the frame, while the card's summary gives way to its detail |
| 4 | `idea` | 0:43 | The episode rewinds; stretto drops in between the agent and the server. The call passes through it; the result stops above it; stretto reads the ids and sends two lookups to the orders drawer; their results clip under the result; the bundle goes to the agent. On "skips those turns", turns 2 and 3 fade and the rest close up, each a beat after the last (14 → 12 on the HUD); "no new tools", "same prompt" | Van Wijk zoom out of the card; a glide down to the timeline and back |
| 5 | `safe` | 1:07 | Close on the server: stretto's reach touches each read drawer, and stops short of the write door, which glows behind its shaking lock. Then a wrong guess: an unused lookup rides into the conversation and breaks into a few tokens (the HUD's tokens tick up, in amber); nothing else changes | Push in to the server; pan to the conversation |
| 6 | `learn` | 1:20 | Recorded sessions in rows; as each passes, its transitions count (+1) into a tally, and dotted lines show where each argument came from. The rows condense into a flow graph whose edges thicken as their counts arrive, each chance settling as the counts do (47 of 50 = 0.94, 22 of 36 = 0.61), with their argument sources; the graph folds into `retail.flow.json`, which opens as a diff, reviewed; a learning curve marks 10 sessions | **3D tilt flyover**: the tableau tips back into a floor and the camera flies over the session stream, then levels out over the graph |
| 7 | `rule` | 1:42 | A balance builds as the wipe clears (the fulcrum, then the beam drawn out from it both ways, then the pans), so no frame after the wipe is empty. A turn saved (≈ 6,000 tokens) drops on one end, a wasted lookup (≈ 2,530) on the other; the fulcrum slides until the beam levels, at δ/(β+δ) ≈ 0.3. The beam stands up into a gauge with the threshold across it; candidate lookups take their places by their counted chances (47/50 = 0.94 … 3/25 = 0.12); those above the line fire, the one below greys out | **Bar wipe**: the mark's three bars sweep across, each before the last |
| 8 | `results` | 2:00 | The bars without stretto draw in as the pan settles; then, for each model, the bar with stretto shrinks in as its number counts: Claude Sonnet 5 −20.5%, Claude Haiku 4.5 −22.4%, GLM-5.3 −27.9%, each landing as the voice names the model | **Whip pan** with motion blur (a directional blur set from the camera's speed) |
| 9 | `prompt` | 2:16 | Left, a prompt ("make independent calls at once"): it can batch only the calls the agent already knows (−3.4%, −5.9%, −7.6%). Right, the prompt plus stretto: the reads the result reveals ride in the same turn (−22.9%, −17.2%) | **Split screen**: the chart lifts away as the halves slide in from above and below |
| 10 | `start` | 2:28 | A terminal types `stretto init --host claude-code --domain notes -- npx @modelcontextprotocol/server-filesystem ./notes` and prints its real output (the `claude mcp add` line and the next steps, scrolling); above it the world reassembles in miniature, and stretto drops in front of the server; sessions flow through it | **Dip** through black |
| 11 | `end` | 2:39 | The miniature agent, stretto and server fly into the mark's three bars, each before the last has landed; the wordmark writes on; "Read ahead of your agent."; `alexnodeland.github.io/stretto · open source, MIT` | A pull back into the mark, with the music's closing motif |

The camera never stops: a small drift in position, zoom and roll runs under every move, dust drifts at three depths, and the grid moves with parallax.

## How timing follows the voice

`main.js` lays the scenes end to end. Each scene has a lead (when its line starts), a least length, and a breath held after the line; it lasts `max(least length, lead + the line's length + breath)`. So the voice sets the pace: a longer line makes a longer scene, and every scene after it moves.

Beats inside a scene are tied to words: `ctx.word('results', 'Haiku')` is when the voice says "Haiku", so the Haiku bar lands then; the order ids light on "two orders"; the turns close up on "skips those turns"; the terminal starts typing on "run stretto init". The manifest's words are what a speech recognizer heard, with its slips ("Strato" and "Streto" for "stretto", "Tao Tube Inch's" for "τ²-bench's", "3 to 8 percent" for "three to eight percent"). `render.mjs` aligns them to the script's own words by edit distance (`alignWords` in `../lib.mjs`), placing a word the aligner missed between its neighbours, and passes the page the script's words with their times (`window.__voice`). The anchors are phrases of the script, so they are always found. Before a line is voiced, it is estimated at 2.5 words a second, so the video can be built and checked without the voice; `--info` names any anchor placed by estimate.

The page publishes what the renderer needs: `window.__duration`, `__marks` (scene starts), `__lines` (when each clip starts), `__events` (sound effects, `{t, sfx, gain}`), `__music` (`{open: [a, b, c], end}`: the three bars' entries, and the closing motif, timed to resolve just after the last word) and `__poster`.

## The sound

- **Voice:** each clip at its scene's start plus its lead, checked not to overlap the next.
- **Music:** `video/music.py --duration D --open a,b,c --end E`, with the timeline's own times: the motif's three entries land with the mark's three bars, and its resolution with the end card. It is set to about −30 LUFS and ducked under the voice with `sidechaincompress`.
- **Effects:** `video/out/sfx/*.wav` at the timeline's events (whooshes for camera moves, pops and clicks for cards and lookups, ticks for counts, a thump when stretto lands, a chime when the flow is reviewed), each file read once and split per use.
- **Loudness:** two passes (measure, then a linear gain) to −16 LUFS. True peaks are held to −2 dBTP in the mix, so they stay under −1.5 dBTP after AAC encoding; the last second fades out. The current render measures −16.0 LUFS and −1.9 dBTP.
- **Captions:** `writeVttWords` (in `../lib.mjs`) splits each line's `text` with `cuePieces`, as `writeVtt` does, and starts each piece when its first word is spoken.

`--audio-only` makes the sound and captions again and puts them into the video there is, copying its frames: for a change to the voice that does not change its length. A longer or shorter voice changes the pacing, and needs a full render.

## Notes

- The terminal's command has `--domain notes`: `stretto init` requires `--domain` (or `--flow`), and without it exits with an error. Its output is what `stretto init` prints for that command.
- The text is sized for a phone, where the video plays 360 to 400 px wide: where the camera holds on them, the labels, names and numbers are about 26 px or more at 1080p (the terminal's and the diff's code excepted), and the wide shots of the tableau are framed at about zoom 1.
- Every number on screen is from `docs/results/claims.md` and `docs/results/frontier-2026-09-27.md`. The flow graph's, the gauge's and the diff's counts are an illustration of one domain, not results.
- The teaser loops: its idle motion turns a whole number of times in 8 seconds (`E.LOOP`), and it holds the server's lights and the lens's sheen still and leaves out the vignette and the wide glows, since a GIF pays for every pixel that changes. The GIF is 64 colours, undithered.
