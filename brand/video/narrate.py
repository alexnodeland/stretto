#!/usr/bin/env python3
"""Voice the brand kit's narration with Kokoro-82M, a local text-to-speech model.

    python3 brand/video/narrate.py [launch] [walkthrough] [explainer]   # default: all three

The lines are in video/launch/narration.json, video/walkthrough/narration.json
(with every caption in walkthrough.cast) and explainer/narration.json. Each
line is spoken by one voice (af_heart) at one level, and written:

- for the videos, as WAV clips and a manifest.json (each line's text and
  length) in video/out/voice/NAME/, which render.mjs reads to pace the video,
  place the clips and write the captions (media/NAME.vtt);
- for the explainer, as explainer/audio/step-N.mp3, which it plays when its
  sound is on.

A clip whose text, voice and speed have not changed is kept. The text on screen
and in the captions is written for reading; SAY turns it into what the voice
should say, and PHONEMES fixes words the phonemizer gets wrong.

Needs: pip install -r brand/video/requirements-narrate.txt; the model files kokoro-v1.0.onnx and
voices-v1.0.bin (github.com/thewh1teagle/kokoro-onnx, release model-files-v1.0)
in $KOKORO_DIR (default: brand/video/out/models); and, for the explainer,
ffmpeg ($FFMPEG, else on PATH, else imageio-ffmpeg's).
"""

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
BRAND = HERE.parent
OUT = HERE / "out" / "voice"

VOICE = "af_heart"
SPEED = 1.0
RATE = 24000
# Every clip at one level: its RMS at LEVEL dBFS, its peaks below PEAK dBFS.
LEVEL = -19.0
PEAK = -1.5

# What the voice should say for text written to be read.
SAY = [
    (r"GLM-5\.3", "GLM five point three"),
    (r"Claude Haiku 4\.5", "Claude Haiku four point five"),
    (r"τ²-bench", "tau two bench"),
    (r"AgentDojo", "Agent Dojo"),
    (r"BFCL", "B F C L"),
    (r"stretto-proxy", "stretto proxy"),
    (r"flow-show", "flow show"),
    (r"flow-diff", "flow diff"),
    (r"--habit-only", "habit only"),
    (r"\b0\.3\b", "zero point three"),
    (r"\bmcp\b", "M C P"),
]
# The phonemizer's flapped "STRED-oh", said as the musical term, with its
# double t held: "STRET-toe".
PHONEMES = [("stɹˈɛɾoʊ", "stɹˈɛt toʊ")]


def spoken(text: str) -> str:
    for pattern, said in SAY:
        text = re.sub(pattern, said, text)
    return text


def lines_of(name: str) -> list[dict]:
    """The lines to voice for `name`: each with a key, the text as written and as said."""
    if name == "launch":
        doc = json.loads((HERE / "launch" / "narration.json").read_text())
        return [{"key": l["key"], "text": l["text"], "say": spoken(l["text"]), "at": l["at"]} for l in doc["lines"]]
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
        return [{"key": l["key"], "text": l["text"], "say": spoken(l["text"])} for l in doc["lines"]]
    raise SystemExit(f"narrate: no narration named {name!r}")


class Voice:
    def __init__(self, models: Path):
        try:
            from kokoro_onnx import Kokoro  # imported here, so --help works without it
        except ImportError:
            raise SystemExit("narrate: pip install -r brand/video/requirements-narrate.txt")

        onnx, voices = models / "kokoro-v1.0.onnx", models / "voices-v1.0.bin"
        for f in (onnx, voices):
            if not f.exists():
                raise SystemExit(f"narrate: {f} is missing (see --help)")
        self.kokoro = Kokoro(str(onnx), str(voices))

    def speak(self, text: str):
        import numpy as np

        phonemes = self.kokoro.tokenizer.phonemize(text, "en-us")
        for wrong, right in PHONEMES:
            phonemes = phonemes.replace(wrong, right)
        samples, rate = self.kokoro.create(phonemes, voice=VOICE, speed=SPEED, is_phonemes=True)
        assert rate == RATE, rate
        samples = np.asarray(samples, dtype=np.float32)
        # Trim silence at the ends, keeping 60 ms.
        loud = np.flatnonzero(np.abs(samples) > 0.01)
        if loud.size:
            keep = int(0.06 * RATE)
            samples = samples[max(0, loud[0] - keep): loud[-1] + keep]
        # One level for every clip.
        rms = float(np.sqrt(np.mean(samples**2))) or 1.0
        gain = 10 ** (LEVEL / 20) / rms
        peak = float(np.max(np.abs(samples))) * gain
        gain *= min(1.0, 10 ** (PEAK / 20) / peak) if peak > 0 else 1.0
        return samples * gain


# Bump when a clip's encoding changes, so every clip is made again.
ENCODING = 2


def fingerprint(line: dict) -> str:
    return hashlib.sha256(json.dumps([line["say"], VOICE, SPEED, LEVEL, PEAK, PHONEMES, ENCODING]).encode()).hexdigest()[:16]


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


def narrate(name: str, voice_of) -> None:
    lines = lines_of(name)
    try:
        import soundfile as sf
    except ImportError:
        raise SystemExit("narrate: pip install -r brand/video/requirements-narrate.txt")
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
            sf.write(wav, voice_of().speak(line["say"]), RATE, subtype="PCM_16")
            # MP3 plays in every browser, including builds without AAC; mono
            # at 48 kb/s is plenty for a voice.
            subprocess.run([ffmpeg(), "-y", "-hide_banner", "-loglevel", "error", "-i", str(wav),
                            "-af", "loudnorm=I=-16:TP=-1.5:LRA=11", "-c:a", "libmp3lame", "-b:a", "48k",
                            "-ac", "1", "-ar", "24000", str(out)], check=True)
            known[line["key"]] = fingerprint(line)
            print(f"explainer: {out.relative_to(BRAND.parent)}")
        stamp.write_text(json.dumps(known, indent=1, sort_keys=True) + "\n")
        return

    dest = OUT / name
    dest.mkdir(parents=True, exist_ok=True)
    manifest = dest / "manifest.json"
    old = {l["key"]: l for l in json.loads(manifest.read_text())["lines"]} if manifest.exists() else {}
    for line in lines:
        wav = dest / f"{line['key']}.wav"
        line["sha"] = fingerprint(line)
        if not (wav.exists() and old.get(line["key"], {}).get("sha") == line["sha"]):
            sf.write(wav, voice_of().speak(line["say"]), RATE, subtype="PCM_16")
        line["file"] = wav.name
        line["duration"] = round(sf.info(wav).duration, 3)
    manifest.write_text(json.dumps({"voice": VOICE, "speed": SPEED, "lines": lines}, indent=1, ensure_ascii=False) + "\n")
    total = sum(l["duration"] for l in lines)
    print(f"{name}: {len(lines)} lines, {total:.1f} s of voice -> {manifest.relative_to(BRAND.parent)}")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0], epilog=__doc__.split("\n\n", 2)[2],
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("names", nargs="*", default=["launch", "walkthrough", "explainer"],
                    help="launch, walkthrough or explainer (default: all three)")
    args = ap.parse_args()
    models = Path(os.environ.get("KOKORO_DIR", HERE / "out" / "models"))
    voice = None

    def voice_of():
        nonlocal voice
        voice = voice or Voice(models)
        return voice

    for name in args.names:
        narrate(name, voice_of)


if __name__ == "__main__":
    sys.exit(main())
