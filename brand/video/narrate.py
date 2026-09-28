#!/usr/bin/env python3
"""Voice the brand kit's narration with Chatterbox, a local text-to-speech model.

    python3 brand/video/narrate.py [explainer-video] [walkthrough] [explainer]   # default: all three

The scripts are video/explainer/narration.json (the explainer video),
video/walkthrough/narration.json (with every caption in walkthrough.cast) and
explainer/narration.json (the interactive explainer). Every line is spoken in
one voice, Chatterbox's own, at one pace and one level, then transcribed back
with faster-whisper and checked against its script; a line with a word missing,
added or garbled is spoken again. The clips are written:

- for the videos, as WAV clips and a manifest.json (each line's text, its
  length and the time of every word) in video/out/voice/NAME/, which
  render.mjs reads to pace the video, place the clips and time the captions;
- for the interactive explainer, as explainer/audio/step-N.mp3, which it plays
  when its sound is on.

A clip whose words and settings have not changed is kept. Text on screen is
written for reading: a line's `say`, and SAY below, give what the voice says.
"stretto" is left to the model, which says it the American way, STRED-oh.

The pace. Chatterbox has no speed setting, and its voice runs fast, about 210
words a minute within a sentence, whatever its settings. So each line is
spoken a sentence at a time (a long sentence split at the clause nearest its
middle); each piece is trimmed of its own silence, slowed toward TARGET_WPM by
no more than SLOWEST (further, speech smears), and joined to the next with a
pause that fits its punctuation.

Needs: pip install -r brand/video/requirements-narrate.txt (its header gives
the order, for torch's CPU build), and ffmpeg ($FFMPEG, else on PATH, else
imageio-ffmpeg's). The models (Chatterbox, about 3.2 GB, MIT; faster-whisper's
small.en, about 0.5 GB) come from Hugging Face on first use and are cached
under $HF_HOME. On a machine busy with other work, set OMP_NUM_THREADS to the
cores that are free: torch's threads contending for busy cores can make it
thirty times slower.
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

ENGINE = "chatterbox-tts 0.1.7"
VOICE = "chatterbox-default"  # the model's own voice (conds.pt); nothing is cloned
RATE = 24000
# Chatterbox's defaults. Lower cfg_weight or exaggeration did not slow this
# voice (cfg_weight 0.15-0.35 with exaggeration 0.35-0.5 all ran within 5% of
# each other), and lower cfg_weight let it change a word.
EXAGGERATION = 0.5
CFG_WEIGHT = 0.5
TEMPERATURE = 0.8
RETRY_TEMPERATURE = 0.6
RETRIES = 3  # takes after the first, each from another seed, while a take fails its check

TARGET_WPM = 160  # words a minute over the whole line, pauses included
FASTEST_WPM = 185  # a take still faster than this, slowed as far as SLOWEST allows, is spoken again
SLOWEST = 0.8  # the most a piece is slowed (ffmpeg atempo); 1 would leave it as spoken
PAUSE_MS = {"sentence": 430, "clause": 210}
LEAD_MS, TAIL_MS = 40, 100  # silence kept before and after each piece
TRIM_MS = 60  # silence kept at each end of a line
SPLIT_WORDS = 20  # a sentence longer than this is split at a clause: long takes slur names
JOIN_WORDS = 5  # a sentence shorter than this is spoken with its neighbour: a word or two alone, it falters
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
        return [{"key": l["key"], "text": l["text"], "say": spoken(l.get("say", l["text"]))} for l in doc["lines"]]
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


def pieces(say: str) -> list[tuple[str, str]]:
    """The line as the takes to speak, each with the pause after it ("sentence" or "clause")."""
    sentences = []
    for s in (s.strip() for s in re.split(r"(?<=[.!?])\s+", say.strip())):
        if s and sentences and len(sentences[-1].split()) < JOIN_WORDS:
            sentences[-1] += " " + s
        elif s:
            sentences.append(s)
    if len(sentences) > 1 and len(sentences[-1].split()) < JOIN_WORDS:
        last = sentences.pop()
        sentences[-1] += " " + last
    out = []
    for sentence in sentences:
        parts = split_long(sentence)
        out += [(p, "clause") for p in parts[:-1]] + [(parts[-1], "sentence")]
    return out


def split_long(sentence: str) -> list[str]:
    """A sentence over SPLIT_WORDS words, split at the clause nearest its middle, and again if need be."""
    words = sentence.split()
    if len(words) <= SPLIT_WORDS:
        return [sentence]
    ends = [i + 1 for i, w in enumerate(words[:-1]) if re.search(r"[,;:]$", w)]
    if not ends:
        return [sentence]
    cut = min(ends, key=lambda i: abs(i - len(words) / 2))
    return split_long(" ".join(words[:cut])) + split_long(" ".join(words[cut:]))


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


class Voice:
    """Chatterbox, loaded once, and faster-whisper to check each take."""

    def __init__(self):
        try:
            import torch  # noqa: F401  (imported here, so --help works without it)
            from chatterbox.tts import ChatterboxTTS
            from faster_whisper import WhisperModel
        except ImportError:
            raise SystemExit("narrate: pip install -r brand/video/requirements-narrate.txt")
        t0 = time.time()
        self.model = ChatterboxTTS.from_pretrained(device="cpu")
        self.whisper = WhisperModel("small.en", device="cpu", compute_type="int8")
        print(f"narrate: models loaded in {time.time() - t0:.0f} s", file=sys.stderr)

    def take(self, text: str, seed: int, temperature: float):
        import numpy as np
        import torch

        torch.manual_seed(seed)
        wav = self.model.generate(text, exaggeration=EXAGGERATION, cfg_weight=CFG_WEIGHT, temperature=temperature)
        return wav.squeeze(0).cpu().numpy().astype(np.float32)

    def speak(self, say: str, seed: int, temperature: float):
        """The line, spoken piece by piece, paced and joined; and how much it was slowed."""
        import numpy as np

        parts = pieces(say)
        takes = [trim(self.take(text, seed + i, temperature), LEAD_MS, TAIL_MS) for i, (text, _) in enumerate(parts)]
        pauses = [PAUSE_MS[after] / 1000 for _, after in parts[:-1]]
        spoken_s = sum(len(t) for t in takes) / RATE
        target_s = len(say.split()) / TARGET_WPM * 60 - sum(pauses)
        tempo = min(1.0, max(SLOWEST, spoken_s / target_s)) if target_s > 0 else 1.0
        out = []
        for i, t in enumerate(takes):
            out.append(atempo(t, tempo))
            if i < len(pauses):
                out.append(np.zeros(int(pauses[i] * RATE), dtype=np.float32))
        return normalize(trim(np.concatenate(out), TRIM_MS, TRIM_MS)), tempo

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
HEARD_AS = [(r"\bin it\b", "init"), (r"\bfile system\b", "filesystem"), (r"\b(clod|clawed)\b", "claude")]


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


def atempo(samples, tempo: float):
    """The samples, slowed (tempo < 1) at the same pitch."""
    import numpy as np
    import soundfile as sf

    if tempo > 0.995:
        return samples
    with tempfile.TemporaryDirectory() as d:
        src, dst = Path(d) / "in.wav", Path(d) / "out.wav"
        sf.write(src, samples, RATE, subtype="FLOAT")
        subprocess.run([ffmpeg(), "-y", "-hide_banner", "-loglevel", "error", "-i", str(src),
                        "-af", f"atempo={tempo:.4f}", "-c:a", "pcm_f32le", str(dst)], check=True)
        return sf.read(dst, dtype="float32")[0].astype(np.float32)


# Bump when the making of a clip changes, so every clip is made again.
ENCODING = 3


def fingerprint(line: dict) -> str:
    settings = [line["say"], ENGINE, VOICE, EXAGGERATION, CFG_WEIGHT, TEMPERATURE, RETRY_TEMPERATURE, TARGET_WPM,
                SLOWEST, PAUSE_MS, LEAD_MS, TAIL_MS, TRIM_MS, SPLIT_WORDS, LEVEL, PEAK, ENCODING]
    return hashlib.sha256(json.dumps(settings, sort_keys=True).encode()).hexdigest()[:16]


def voice_line(voice: Voice, line: dict, wav: Path) -> list[dict]:
    """Speaks the line into `wav` until a take passes its check, else keeps the closest take; returns its words."""
    import soundfile as sf

    seed = int(hashlib.sha256(line["key"].encode()).hexdigest()[:8], 16) % 1_000_000
    best = None
    for attempt in range(1 + RETRIES):
        samples, tempo = voice.speak(line["say"], seed + 1000 * attempt, TEMPERATURE if attempt == 0 else RETRY_TEMPERATURE)
        sf.write(wav, samples, RATE, subtype="PCM_16")
        off, words, heard = voice.check(wav, line["say"])
        wpm = len(line["say"].split()) / (len(samples) / RATE) * 60
        off += max(0.0, wpm - FASTEST_WPM) / 100
        print(f"narrate: {line['key']}: take {attempt + 1}, {len(samples) / RATE:.2f} s ({wpm:.0f} words a minute),"
              f" slowed to {tempo:.2f}" + ("" if off < PASS else f"; failed its check ({off:.2f} off: heard {heard!r})"),
              file=sys.stderr)
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
