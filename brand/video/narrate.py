#!/usr/bin/env python3
"""Voice the brand kit's narration with Kokoro, a local text-to-speech model.

    python3 brand/video/narrate.py [explainer-video] [walkthrough] [explainer]   # default: all three

The scripts are video/explainer/narration.json (the explainer video, as
chapters of lines), video/walkthrough/narration.json (with every caption in
walkthrough.cast) and explainer/narration.json (the interactive explainer).
Every line is spoken in one voice, Kokoro-82M's `af_heart` at one pace, then
transcribed back with faster-whisper and checked against its script; a line
with a word missing, added or garbled is spoken again a touch slower or
faster, which is enough to change Kokoro's take. The clips are written:

- for the videos, as WAV clips and a manifest.json (each line's text, its
  length and the time of every word) in video/out/voice/NAME/, which
  render.mjs reads to pace the video, place the clips and time the captions;
- for the interactive explainer, as explainer/audio/step-N.mp3, which it plays
  when its sound is on.

A clip whose words and settings have not changed is kept. Text on screen is
written for reading: a line's `say`, and SAY below, give what the voice says.
"stretto" is said the American way, STRED-oh, which misaki (Kokoro's
grapheme-to-phoneme front end) gives it from its own rules; LEXICON pins it
anyway, with any other word whose reading must not drift.

The voice and the pace are those of the Auracle films (PACING.md): Kokoro's
`af_heart` at speed 0.85, each line spoken whole so its commas keep their
prosody. Kokoro runs a line in about half its length on a CPU.

Needs: pip install -r brand/video/requirements-narrate.txt (its header gives
the order, for torch's CPU build), and ffmpeg ($FFMPEG, else on PATH, else
imageio-ffmpeg's) for the interactive explainer's MP3s. The models (Kokoro-82M
and its voices, about 0.35 GB, Apache-2.0; faster-whisper's small.en, about
0.5 GB) come from Hugging Face on first use and are cached under $HF_HOME.
"""

import argparse
import difflib
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
BRAND = HERE.parent
OUT = HERE / "out" / "voice"

ENGINE = "kokoro 0.9.4 (Kokoro-82M v1.0, misaki 0.9.4)"
VOICE = "af_heart"
REPO_ID = "hexgrad/Kokoro-82M"
RATE = 24000
SPEED = 0.85  # between Auracle's launch film (0.82) and its films for engineers (0.9); PACING.md
RETRY_SPEEDS = [0.83, 0.87, 0.81]  # a take that fails its check is spoken again at these
# Words whose reading is pinned, in misaki's phoneme alphabet (US voices).
LEXICON = {
    "stretto": "stɹˈɛTO",
}
TRIM_MS = 60  # silence kept at each end of a line
LEVEL = -19.0  # every clip's RMS, dBFS
PEAK = -1.5  # and its peaks below this

# What the voice should say for text written to be read.
SAY = [
    (r"τ²-bench", "tau two bench"),
    (r"\bGLM-5\.3\b", "G L M five point three"),
    (r"\bClaude Haiku 4\.5\b", "Claude Haiku four point five"),
    (r"\bSonnet 5\b", "Sonnet five"),
    (r"AgentDojo", "Agent Dojo"),
    (r"\bBFCL\b", "B F C L"),
    (r"\bLLM\b", "L L M"),
    (r"\bMCP\b", "M C P"),
    (r"\bmcp\b", "M C P"),
    (r"stretto-proxy", "stretto proxy"),
    (r"flow-show", "flow show"),
    (r"flow-diff", "flow diff"),
    (r"--habit-only", "habit only"),
    (r"\b0\.3\b", "zero point three"),
]


def spoken(text: str) -> str:
    for pattern, said in SAY:
        text = re.sub(pattern, said, text)
    return text


def lines_of(name: str) -> list[dict]:
    """The lines to voice for `name`: each with a key, the text as written and as said."""
    if name == "explainer-video":
        doc = json.loads((HERE / "explainer" / "narration.json").read_text())
        return [{"key": l["key"], "text": l["text"], "say": spoken(l.get("say", l["text"]))}
                for beat in doc["beats"] for l in beat["lines"]]
    if name == "walkthrough":
        doc = json.loads((HERE / "walkthrough" / "narration.json").read_text())
        out = [{"key": "intro", "text": doc["intro"], "say": spoken(doc["intro"])}]
        seen = set()
        rows = (HERE / "walkthrough" / "walkthrough.cast").read_text().splitlines()[1:]
        for row in rows:
            _, kind, data = json.loads(row)
            if kind != "m":
                continue
            caption = json.loads(data).get("caption")
            if caption is None or caption in seen:
                continue
            seen.add(caption)
            say = doc.get("say", {}).get(caption, caption)
            out.append({"key": f"caption-{len(seen)}", "text": caption, "say": spoken(say)})
        out.append({"key": "outro", "text": doc["outro"], "say": spoken(doc["outro"])})
        return out
    if name == "explainer":
        doc = json.loads((BRAND / "explainer" / "narration.json").read_text())
        return [{"key": l["key"], "text": l["text"], "say": spoken(l.get("say", l["text"]))} for l in doc["lines"]]
    raise SystemExit(f"narrate: no narration named {name!r}")



def ffmpeg() -> str:
    if os.environ.get("FFMPEG"):
        return os.environ["FFMPEG"]
    if shutil.which("ffmpeg"):
        return "ffmpeg"
    try:
        import imageio_ffmpeg

        return imageio_ffmpeg.get_ffmpeg_exe()
    except ImportError:
        raise SystemExit("narrate: no ffmpeg: set FFMPEG, put ffmpeg on PATH, or pip install imageio-ffmpeg")




def with_lexicon(text: str) -> str:
    """The text with each LEXICON word as misaki's inline markup, [word](/phonemes/)."""
    for word, ps in LEXICON.items():
        text = re.sub(rf"\b{re.escape(word)}\b", lambda m: f"[{m.group(0)}](/{ps}/)", text, flags=re.I)
    return text


class Voice:
    """Kokoro, loaded once, and faster-whisper to check each take."""

    def __init__(self):
        try:
            import torch  # noqa: F401  (imported here, so --help works without it)
            from faster_whisper import WhisperModel
            from kokoro import KPipeline
        except ImportError:
            raise SystemExit("narrate: pip install -r brand/video/requirements-narrate.txt")
        t0 = time.time()
        self.pipeline = KPipeline(lang_code="a", repo_id=REPO_ID)
        self.whisper = WhisperModel("small.en", device="cpu", compute_type="int8")
        print(f"narrate: models loaded in {time.time() - t0:.0f} s", file=sys.stderr)

    def speak(self, say: str, speed: float):
        """The line, spoken whole (Kokoro splits only past its 510-phoneme window), trimmed and levelled."""
        import numpy as np

        audio = [r.audio.numpy() for r in self.pipeline(with_lexicon(say), voice=VOICE, speed=speed) if r.audio is not None]
        if not audio:
            raise SystemExit(f"narrate: Kokoro said nothing for {say!r}")
        return normalize(trim(np.concatenate(audio).astype(np.float32), TRIM_MS, TRIM_MS))

    def check(self, wav: Path, say: str) -> tuple[float, list[dict], str]:
        """Transcribes the clip and compares it with what it should say: (how far off, its words, what was heard).

        How far off is the share of words missing, added or changed, plus one if
        a phrase was dropped or "stretto" was not heard; under PASS, the take passes."""
        import numpy as np
        import soundfile as sf

        # Heard from a clip that starts on its first word, small.en can drop
        # that word; a little silence before it keeps it.
        samples, rate = sf.read(wav, dtype="float32")
        with tempfile.TemporaryDirectory() as d:
            padded = Path(d) / "padded.wav"
            sf.write(padded, np.concatenate([np.zeros(int(CHECK_PAD * rate), dtype=np.float32), samples]), rate)
            segments = list(self.whisper.transcribe(str(padded), word_timestamps=True, beam_size=5)[0])
        heard = " ".join(s.text.strip() for s in segments)
        words = [{"w": w.word.strip(), "start": round(max(0.0, float(w.start) - CHECK_PAD), 3),
                  "end": round(max(0.0, float(w.end) - CHECK_PAD), 3)} for s in segments for w in s.words]
        ref, hyp = comparable(say), comparable(heard)
        ops = difflib.SequenceMatcher(None, ref, hyp).get_opcodes()
        wer = sum(max(i2 - i1, j2 - j1) for tag, i1, i2, j1, j2 in ops if tag != "equal") / max(1, len(ref))
        # Two or more words in a row not said is a dropped phrase, not a
        # mishearing, however long the line: it fails the take.
        dropped = max([(i2 - i1) - (j2 - j1) for tag, i1, i2, j1, j2 in ops if tag in ("delete", "replace")] + [0])
        wer += dropped >= 2
        # small.en hears "tau two bench" as "how to bench", whoever says it.
        if "tau two bench" in say.lower():
            wer = max(0.0, wer - 2 / max(1, len(ref)))
        return wer + ("stretto" in ref and "stretto" not in hyp), words, heard


PASS = 0.12
CHECK_PAD = 0.3  # seconds of silence before a clip, as it is checked
# How small.en writes the coined "stretto" when it is said right.
STRETTO_HEARD = {"stretto", "stredo", "streddo", "stretoh", "strato", "stretteau", "streto", "stretta"}
NUMBERS = {"zero": 0, "one": 1, "two": 2, "three": 3, "four": 4, "five": 5, "six": 6, "seven": 7, "eight": 8,
           "nine": 9, "ten": 10, "eleven": 11, "twelve": 12, "thirteen": 13, "fourteen": 14, "fifteen": 15,
           "sixteen": 16, "seventeen": 17, "eighteen": 18, "nineteen": 19, "twenty": 20, "thirty": 30, "forty": 40,
           "fifty": 50, "sixty": 60, "seventy": 70, "eighty": 80, "ninety": 90}


# How small.en writes a few of the scripts' words, whoever says them.
HEARD_AS = [(r"\bin it\b", "init"), (r"\bfile system\b", "filesystem"), (r"\b(clod|clawed)\b", "claude"),
            (r"\bwalk through\b", "walkthrough"), (r"\bstrato ?proxy\b", "stretto proxy")]


def comparable(text: str) -> list[str]:
    """Words to compare, as small.en writes them: numbers in digits, spelled-out letters run together."""
    text = re.sub(r"[^a-z0-9' ]", " ", text.lower().replace("%", " percent"))
    for heard, meant in HEARD_AS:
        text = re.sub(heard, meant, text)
    words = text.split()
    out, i = [], 0
    while i < len(words):
        w, s = words[i], ""
        if w.endswith("'s") and w[:-2] in NUMBERS:
            w, s = w[:-2], "'s"
        if w in NUMBERS:
            n = NUMBERS[w]
            if n >= 20 and i + 1 < len(words) and NUMBERS.get(words[i + 1], 10) < 10:
                n, i = n + NUMBERS[words[i + 1]], i + 1
            out.append(f"{n}{s}")
        elif len(w) == 1 and w.isalpha() and i + 1 < len(words) and len(words[i + 1]) == 1 and words[i + 1].isalpha():
            j = i
            while j < len(words) and len(words[j]) == 1 and words[j].isalpha():
                j += 1
            out.append("".join(words[i:j]))
            i = j - 1
        elif w in STRETTO_HEARD:
            out.append("stretto")
        elif w not in ("percent", "point"):
            out.append(w)
        i += 1
    return out


def trim(samples, lead_ms: float, tail_ms: float):
    """The samples from lead_ms before the first sound to tail_ms after the last (-45 dB of the peak)."""
    import numpy as np

    peak = float(np.max(np.abs(samples))) if samples.size else 0.0
    loud = np.flatnonzero(np.abs(samples) > peak * 10 ** (-45 / 20)) if peak > 0 else np.array([])
    if not loud.size:
        return samples
    return samples[max(0, loud[0] - int(lead_ms / 1000 * RATE)): loud[-1] + int(tail_ms / 1000 * RATE)]


def normalize(samples):
    import numpy as np

    rms = float(np.sqrt(np.mean(samples**2))) or 1.0
    gain = 10 ** (LEVEL / 20) / rms
    peak = float(np.max(np.abs(samples))) * gain
    gain *= min(1.0, 10 ** (PEAK / 20) / peak) if peak > 0 else 1.0
    return samples * gain



# Bump when the making of a clip changes, so every clip is made again.
ENCODING = 4


def fingerprint(line: dict) -> str:
    settings = [line["say"], ENGINE, VOICE, SPEED, RETRY_SPEEDS, LEXICON, TRIM_MS, LEVEL, PEAK, ENCODING]
    return hashlib.sha256(json.dumps(settings, sort_keys=True).encode()).hexdigest()[:16]


def voice_line(voice: Voice, line: dict, wav: Path) -> list[dict]:
    """Speaks the line into `wav` until a take passes its check, else keeps the closest take; returns its words."""
    import soundfile as sf

    best = None
    for speed in [SPEED] + RETRY_SPEEDS:
        samples = voice.speak(line["say"], speed)
        sf.write(wav, samples, RATE, subtype="PCM_16")
        off, words, heard = voice.check(wav, line["say"])
        wpm = len(line["say"].split()) / (len(samples) / RATE) * 60
        print(f"narrate: {line['key']}: speed {speed}, {len(samples) / RATE:.2f} s ({wpm:.0f} words a minute)"
              + ("" if off < PASS else f"; failed its check ({off:.2f} off: heard {heard!r})"), file=sys.stderr)
        if off < PASS:
            return words
        if best is None or off < best[0]:
            best = (off, samples, words)
    print(f"narrate: WARNING {line['key']}: no take passed its check; keeping the closest ({best[0]:.2f} off)",
          file=sys.stderr)
    sf.write(wav, best[1], RATE, subtype="PCM_16")
    return best[2]


def narrate(name: str, voice_of) -> None:
    try:
        import soundfile as sf
    except ImportError:
        raise SystemExit("narrate: pip install -r brand/video/requirements-narrate.txt")

    lines = lines_of(name)
    if name == "explainer":
        dest = BRAND / "explainer" / "audio"
        dest.mkdir(parents=True, exist_ok=True)
        stamp = dest / "sources.json"
        known = json.loads(stamp.read_text()) if stamp.exists() else {}
        for line in lines:
            out = dest / f"{line['key']}.mp3"
            if out.exists() and known.get(line["key"]) == fingerprint(line):
                continue
            wav = OUT / "explainer" / f"{line['key']}.wav"
            wav.parent.mkdir(parents=True, exist_ok=True)
            voice_line(voice_of(), line, wav)
            # MP3 plays in every browser, including builds without AAC; mono
            # at 48 kb/s is plenty for a voice.
            subprocess.run([ffmpeg(), "-y", "-hide_banner", "-loglevel", "error", "-i", str(wav),
                            "-af", "loudnorm=I=-16:TP=-1.5:LRA=11", "-c:a", "libmp3lame", "-b:a", "48k",
                            "-ac", "1", "-ar", "24000", str(out)], check=True)
            known[line["key"]] = fingerprint(line)
            stamp.write_text(json.dumps(known, indent=1, sort_keys=True) + "\n")
            print(f"explainer: {out.relative_to(BRAND.parent)}")
        return

    dest = OUT / name
    dest.mkdir(parents=True, exist_ok=True)
    manifest = dest / "manifest.json"
    old = {l["key"]: l for l in json.loads(manifest.read_text())["lines"]} if manifest.exists() else {}
    for line in lines:
        wav = dest / f"{line['key']}.wav"
        line["sha"] = fingerprint(line)
        kept = old.get(line["key"], {})
        if wav.exists() and kept.get("sha") == line["sha"] and kept.get("words"):
            line["words"] = kept["words"]
        else:
            line["words"] = voice_line(voice_of(), line, wav)
        line["file"] = wav.name
        line["duration"] = round(sf.info(wav).duration, 3)
    manifest.write_text(json.dumps({"voice": VOICE, "engine": ENGINE, "lines": lines}, indent=1, ensure_ascii=False) + "\n")
    total = sum(l["duration"] for l in lines)
    words = sum(len(l["say"].split()) for l in lines)
    print(f"{name}: {len(lines)} lines, {total:.1f} s of voice, {words / total * 60:.0f} words a minute"
          f" -> {manifest.relative_to(BRAND.parent)}")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0], epilog=__doc__.split("\n\n", 2)[2],
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("names", nargs="*", default=["explainer-video", "walkthrough", "explainer"],
                    help="explainer-video, walkthrough or explainer (default: all three)")
    args = ap.parse_args()
    voice = None

    def voice_of():
        nonlocal voice
        voice = voice or Voice()
        return voice

    for name in args.names:
        narrate(name, voice_of)


if __name__ == "__main__":
    sys.exit(main())
