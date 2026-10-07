# The explainer video

A narrated explainer of stretto, 3 minutes 20 seconds long with the current voice-over: ten chapters between the title and an end card, each one diagram on a still stage, built up as the voice names its parts. Its pacing and look follow Auracle's films ([`../PACING.md`](../PACING.md)). It renders to `brand/media/`:

| File | What it is |
|---|---|
| `explainer.mp4` | 1920 × 1080, 30 fps, H.264 High with faststart (`h264Args`, tuned below) and AAC sound: the voice and the music bed; 10.9 MB with the current voice-over (it must stay under 20 MB) |
| `explainer.vtt` | Captions: each line's `text` from `narration.json`, timed by the voice's own words |
| `explainer-poster.png` | The frame where the agent's one result carries the two reads stretto made, and its turns have closed up from five to three (`window.__poster`) |
| `explainer-teaser.gif`, `.webm` | An 8-second loop at 1280 × 720, for the README: a `get_user_details` result comes back with the proxy's own block under it, `--- Also looked up automatically (current results; no need to repeat these calls) ---`, and the two `get_order_details` reads in it; the agent's five turns close up to three. It is drawn only for the teaser: the film opens on its title |

## Rendering

One command makes everything from the voice-over's manifest (`video/out/voice/explainer-video/manifest.json`): the timeline and its anchors, the music's cues, the captions, the poster and the teaser. After a new voice-over, run it again; nothing needs editing by hand.

```sh
cd brand
PLAYWRIGHT_BROWSERS_PATH=/path/to/playwright-browsers \
FFMPEG=/path/to/ffmpeg \
PYTHON=/path/to/python-with-numpy-and-soundfile \
node video/film.mjs explainer
```

`PLAYWRIGHT_BROWSERS_PATH` is needed only if Playwright's Chromium is not in its default place; `FFMPEG` only if ffmpeg is not on PATH (else imageio-ffmpeg's is used); `PYTHON` is the Python that runs `video/music.py` (it needs numpy and soundfile; without it the mix has no music, and the renderer says so). On 4 CPUs it takes about 5 minutes for the video's 6,014 frames with the default two pages, and under half a minute for the teaser.

```sh
node video/film.mjs explainer --info                  # the timeline the voice sets, without rendering: scene starts, cues, anchors not found
node video/film.mjs explainer --only video            # explainer.mp4, explainer.vtt, explainer-poster.png
node video/film.mjs explainer --only teaser           # explainer-teaser.gif and explainer-teaser.webm
node video/film.mjs explainer --audio-only            # the sound and captions again, into the explainer.mp4 there is
node video/film.mjs explainer --poster-only           # explainer-poster.png alone, without a caption
node video/film.mjs explainer --stills 12,48.5 --width 960   # single frames, to video/out/stills/
node video/film.mjs explainer --preview 88,95         # a silent stretch, to video/out/preview.mp4
```

Options: `--workers N` (Chromium pages drawing frames at once; default CPUs − 1, at most 2), `--crf N` (default 26), `--fps`, `--poster T`, `--cut teaser` (for `--stills` and `--preview`). The mix is written to `video/out/explainer/mix.wav` and the music to `video/out/music/explainer-video.wav`; nothing is written into `video/out/voice/`.

The video goes into git and onto GitHub Pages, so it is encoded to stay under 20 MB: `h264Args` (High, BT.709, yuv420p, tune animation, faststart) at CRF 26 (`--crf`), preset veryslow, adaptive quantization mode 3 (it spends bits on dark, flat areas), and an error-diffused conversion from the page's RGB to limited-range YUV, so the dark ground and its faint glow do not band. Checked against the page's own frames with their darks lifted threefold, the encode shows the faint steps the page's 8-bit gradients already have, and at normal contrast none.

Every frame is drawn by `window.__render(t)`, screenshotted by headless Chromium and piped into ffmpeg's stdin (`../lib.mjs`). No frame is written to disk. Several pages draw frames at once, and ffmpeg receives them in order. A frame is a pure function of `t`: no CSS transitions or animations, no timers, and randomness only from seeded generators (`ST.rng`), so the pages can draw frames out of order and the result is the same. To preview in a browser, serve `brand/` over HTTP and open `video/explainer/index.html?play`, or `?t=48.5` for one frame; the page then reads `narration.json` itself and estimates the voice's timing.

## The files

| File | What it holds |
|---|---|
| `narration.json` | The script: chapters (`beats`), each with a `lead`, a `tail`, a least length `min`, and its lines: `text` (captions), `say` (what the voice says), `em` (the phrases lit in petrol), `post` (the pause after) |
| `index.html` | The page: the shared stage (`../stage/`) with this film's chapters |
| `film.js` | The chapters, from the title to the end card, and the teaser |
| `../stage/` | What the films share: `style.css` (the dot grid, the layers, the brand's dark palette), `logo.js` (the wordmark's outline, from `brand/logo/stretto-wordmark-dark.svg`), `stage.js` (easing, a seeded generator, and the pieces: cards, chips, arrows, packets, code, typing, bar charts, captions) and `main.js` (the timeline the voice sets, the frame's furniture, and `window.__render`) |
| `../film.mjs` | The renderer, for any film on the stage: frames, the sound mix, captions, poster, and this film's teaser |

## The chapters

| # | Chapter | What the frame shows |
|---|---|---|
| | `title` | The mark's three bars enter one after another, each before the last has finished, with the music's three entries; the wordmark; "Read ahead of your agent." |
| 01 | `turns` | The conversation grows on the left; on the right, each LLM turn reads the whole conversation (a bar, longer each turn) and picks one step |
| 02 | `decided` | The first result's record: the two order ids light, and arrows run to the next two turns, which read them; each is typed out again, one LLM turn each |
| 03 | `idea` | The agent, `stretto-proxy` and the MCP server. The call goes through; stretto makes the two reads; one result rides back with them in it; the turns close up, five to three; no new tools, the same prompt |
| 04 | `reads` | Some of τ²-bench retail's tools with their read and write marks: a flow may call the reads, not the writes. A detour costs tokens and leaves the state as it was |
| 05 | `learn` | Recorded sessions; what `stretto learn` counts after each call and where each argument came from, with the counts of `docs/examples/retail-10-sessions.flow.json`; `stretto flow-diff` between five sessions and ten; the replay result for ten of an agent's own sessions |
| 06 | `console` | The console's own screens (`brand/media/console/`): the overview, a served session with the lookups the flow made and why, and a flow's graph at a threshold |
| 07 | `rule` | The two costs, a turn saved and a detour (live, GLM-5.3: 6,000 and 2,530 input tokens), the line between them at about 0.3, and example lookups on the chance they are used before the next write: those above are made |
| 08 | `results` | Fewer LLM turns, live on 28 τ²-bench retail and airline tasks: Claude Sonnet 5 20.5%, Claude Haiku 4.5 22.4%, GLM-5.3 27.9%, each with its interval and scope |
| 09 | `prompt` | Anthropic's sample prompt for parallel tool calls (3.4%, 5.9%, 7.6%, with their intervals; not all of it established) against the prompt plus a flow, for the two Claude models (22.9%, 17.2%) |
| 10 | `start` | A terminal: `cargo install`, then `stretto init` and the `claude mcp add` line it prints |
| | `end` | The lockup, the tagline, the docs and the repository, and the music's closing motif |

## How timing follows the voice

`../stage/main.js` lays the chapters end to end from the voice's own clips: each waits its `lead`, holds each line for as long as the voice takes to say it plus its `post`, then its `tail`, and lasts at least `min`. A longer line makes a longer chapter, and every chapter after it moves.

Beats inside a chapter are tied to words: `ctx.at('idea4', 'skips')` is when the voice says "skips", so the turns close up then. The manifest's words are what a speech recognizer heard; `../film.mjs` aligns them to the spoken script by edit distance (`alignWords` in `../lib.mjs`) and passes the page the script's words with their times (`window.__voice`). A caption's written words take their times from the spoken ones, one for one where they match, else by their share of the line (a line whose numbers are written as digits). Before a line is voiced, it is estimated at 2.6 words a second, so the video can be built and checked without the voice; `--info` names any anchor placed by estimate.

The page publishes what the renderer needs: `window.__duration`, `__marks` (chapter starts), `__lines` (when each clip starts), `__music` (`{open: [a, b, c], end}`: the three bars' entries, and the closing motif just after the last word), `__poster`, and `__events`, which is empty: there are no sound effects.

## The sound

- **Voice:** Kokoro `af_heart` at speed 0.85 (`../narrate.py`), each clip at its line's start.
- **Music:** `video/music.py --duration D --open a,b,c --end E`, with the timeline's own times: the motif's three entries land with the mark's three bars, and its resolution with the end card. It is set to about −30 LUFS and ducked under the voice with `sidechaincompress`.
- **Loudness:** two passes (measure, then a linear gain) to −16 LUFS. True peaks are held to −2 dBTP in the mix, so they stay under −1.5 dBTP after AAC encoding; the last second fades out.
- **Captions:** `writeVttWords` (in `../lib.mjs`) splits each line's `text` with `cuePieces`, as `writeVtt` does, and starts each piece when its first word is spoken.

`--audio-only` makes the sound and captions again and puts them into the video there is, copying its frames: for a change to the voice that does not change its length. A longer or shorter voice changes the pacing, and needs a full render.

## Notes

- Every number on screen is from `docs/results/claims.md` or `brand/messaging.md`, with its scope beside it. The flow's counts are those of `docs/examples/retail-10-sessions.flow.json`; the chances on the rule's line are marked as illustrative, not measured. The `flow-diff` card is the command's own output for five sessions against ten (`docs/review.md`).
- The terminal's command has `--domain notes`: `stretto init` requires `--domain` (or `--flow`). Its output is what `stretto init` prints for that command, as the walkthrough recorded it (the shell expands `~/notes` to `/home/me/notes`).
- The text is sized for a phone, where the video plays 360 to 400 px wide: the captions are 40 px at 1080p, and the labels 17 px or more.
