# Pacing: notes from Auracle's films

stretto's explainer was rebuilt in October 2026 on the model of the films of [Auracle](https://github.com/alexnodeland/auracle) (`www/video/`), a sibling project whose illustrated films (launch, engine, math, dsp, taste) are paced well. These notes record what those films do, measured from their `timeline.json` and `script.json`, and how the explainer applies it. They hold for any narrated video in this kit.

## What the Auracle films measure

| Film | Length | Lines | Chapters | Words | Speech share | Words a minute while speaking | Words a minute over the film | Words a line (median) | Gap between lines (median) |
|---|---|---|---|---|---|---|---|---|---|
| launch | 98 s | 17 | 9 | 169 | 59% | 175 | 103 | 10 | 0.95 s |
| engine | 137 s | 16 | 11 | 307 | 86% | 156 | 134 | 19 | 0.75 s |
| math | 166 s | 34 | 11 | 379 | 82% | 167 | 137 | 11 | 0.40 s |
| dsp | 169 s | 32 | 11 | 390 | 83% | 167 | 139 | 12.5 | 0.40 s |
| taste | 109 s | 17 | 10 | 276 | 82% | 185 | 153 | 14 | 1.02 s |

The old stretto explainer, for comparison: 166 s, **11 lines** of up to **61 words** each, one line a scene, 386 words. The words a minute were about the same (140). The difference was the grain: a scene held one long paragraph while the picture tried to keep up, and the captions changed once every 15 seconds.

## The rules the films follow

From Auracle's `www/brand/voice.md` (the spoken voice) and its timelines:

1. **One breath a sentence.** 16 words at most, usually fewer. Subject first, new idea last: the voice lands at the end of a sentence, so the point goes there.
2. **A line is one or two sentences, and the picture changes with it.** A chapter is 10 to 15 seconds (median 12) and holds two to four lines. Every line has something on screen that arrives on its key word.
3. **The voice is Kokoro `af_heart`, unhurried.** Speed 0.82 for the launch film, 0.9 for the films for engineers. Lines are rendered whole, so Kokoro's own prosody carries the commas.
4. **Silence is written in.** About 0.4 s after a line (0 to 1 s), 0.2 s before and after a chapter, and longer breaths where an idea needs to land: 0.7 to 1.2 s after a result or a number.
5. **A cold open before any words.** The film's clearest moment, shown with no voice, then the entrance mark with the title. The first line comes about two seconds after the mark, and says what the film will show, not "This is...".
6. **Captions are part of the picture.** The line is set large at the foot of the frame, each word lit as it is spoken, the terms that matter in the accent color. With the sound off, the film still reads.
7. **No sound effects.** The voice, a soft bed under it, and two marks: one entering, one leaving. Nothing whooshes.
8. **The picture is flat, steady and exact.** No camera flights, no glow for its own sake. Diagrams are drawn from the real thing (the code, the data, the output) and labelled in a monospace face. Motion shows only what the system does.
9. **The ending is one thing to try**, then the lockup about two seconds after the last word.

## How the explainer applies them

- **Voice:** Kokoro `af_heart` (`narrate.py`), the same model and voice as Auracle, checked line by line by faster-whisper. At 0.9 its short lines ran at 179 words a minute while speaking, brisker than Auracle's films for engineers (156 to 167), so it speaks at 0.85: 170 words a minute.
- **Script:** `explainer/narration.json` is a list of chapters (`beats`), each with its lines, a `lead` before its first line, a `post` pause after each line and a `tail` after its last. Every line is one or two sentences of at most 16 words. Its 37 lines run at 170 words a minute while speaking. Its chapters run longer than Auracle's, 12.6 to 24.0 seconds (median 18.8), since each holds three or four lines about one diagram; the diagram changes with each line instead.
- **Picture:** one diagram per chapter on a still stage, built up as the words name its parts (`film.js`, timed by the voice's own words). The captions are set at the foot of the frame and lit word by word. What is shown is the real thing: the proxy's own wording for the reads it adds to a result, τ²-bench retail's tools, the counts of a flow in `docs/examples/`, and the console's own screens.
- **Sound:** the voice and the music bed from `music.py`, with the mark's three entries at the start and the closing motif at the end. No effects.

## The deep dives

The math film and the console film (`math/`, `console/`) are drawn on the same stage, in the same voice and at the same pace, with the same grammar: a cold open, the lockup, chapters of one or two short lines each, and the end card. The console film's picture is the real console, captured by driving it (`console/capture.sh`); its camera moves in on what each line names, and a cursor goes to each control the capture clicked.
