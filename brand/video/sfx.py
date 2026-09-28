#!/usr/bin/env python3
"""Procedural, license-free UI/transition sound effects for the explainer
video - synthesized from scratch (noise, sines and the filters/envelopes
music.py already defines), nothing sampled.

    python3 brand/video/sfx.py               # renders all nine
    python3 brand/video/sfx.py pop chime      # or a subset, by name

Writes 48 kHz stereo (mono content, doubled) WAVs to brand/video/out/sfx/,
each peak-normalized to about -6 dBFS: present enough to read as an accent,
quiet enough to sit under the music/narration without fighting them. Every
sound is well under a second - "short, subtle, tasteful, non-fatiguing" is
the brief, so nothing here loops, rings on, or draws attention to itself.

  whoosh-soft  a gentle pass-by: filtered noise, muffled-to-bright, swelling
               and fading (symmetric - no strong in/out direction)
  whoosh-fast  the same idea, shorter and a touch brighter - a quicker beat
  swoosh-in    zooming into something: noise sweeping muffled to bright,
               building to an "arrival" at the very end, not the middle
  pop          a card landing: a fast downward pitch glide plus a light
               high click, like a soft-plastic tap
  tick         a counter step: a very short, high, dry blip
  click        typing: a shorter, drier, broadband click than tick
  chime        a lookup fires/succeeds: two bell partials (root, fifth),
               the second arriving just after the first - D major, the
               score's key, so it reads as part of the same world
  thump        a scene transition: a soft low downward-gliding tone with a
               little noise body, felt more than heard
  shimmer      a reveal: a handful of bright, high partials (D major, high
               octave) entering in a quick ascending sparkle with a long,
               soft decay

Needs numpy and soundfile (brand/video/requirements-narrate.txt).
"""

import hashlib
import sys
from pathlib import Path

import numpy as np
import soundfile as sf

sys.path.insert(0, str(Path(__file__).resolve().parent))
from music import SR, bell, env_pluck, hz, lowpass  # noqa: E402

OUT = Path(__file__).resolve().parent / "out" / "sfx"
PEAK_DB = -6.0


def _stereo(mono: np.ndarray) -> np.ndarray:
    return np.stack([mono, mono], axis=1).astype(np.float32)


def _norm(x: np.ndarray, peak_db: float = PEAK_DB) -> np.ndarray:
    peak = float(np.max(np.abs(x))) or 1.0
    return x * (10 ** (peak_db / 20) / peak)


def _glide_tone(n: int, f0: float, f1: float, sr: int = SR) -> np.ndarray:
    """A sine whose frequency glides exponentially from f0 to f1 over n samples."""
    t = np.arange(n) / sr
    dur = n / sr
    freq = f0 * (f1 / f0) ** (t / max(dur, 1e-9))
    phase = 2 * np.pi * np.cumsum(freq) / sr
    return np.sin(phase)


def _swept_noise(n: int, f_lo: float, f_hi: float, rng: np.random.Generator,
                  rising: bool, ease: float) -> np.ndarray:
    """Filtered noise that crossfades from a muffled (f_lo) to a bright
    (f_hi) copy of itself, standing in for a moving bandpass sweep without
    needing a true time-varying filter."""
    noise = rng.standard_normal(n)
    lo = lowpass(noise, f_lo)
    hi = lowpass(noise, f_hi)
    t = np.linspace(0, 1, n)
    mix = (t if rising else 1 - t) ** ease
    return lo * (1 - mix) + hi * mix


def whoosh_soft(rng: np.random.Generator) -> np.ndarray:
    n = int(0.55 * SR)
    band = _swept_noise(n, 500, 3800, rng, rising=True, ease=1.0)
    t = np.linspace(0, 1, n)
    env = np.sin(np.pi * t ** 0.85)  # soft swell, peak near the middle
    return band * env


def whoosh_fast(rng: np.random.Generator) -> np.ndarray:
    n = int(0.28 * SR)
    band = _swept_noise(n, 700, 5200, rng, rising=True, ease=0.8)
    t = np.linspace(0, 1, n)
    env = np.sin(np.pi * t ** 0.7)
    return band * env * 1.1


def swoosh_in(rng: np.random.Generator) -> np.ndarray:
    n = int(0.42 * SR)
    band = _swept_noise(n, 450, 6000, rng, rising=True, ease=1.4)
    t = np.linspace(0, 1, n)
    env = np.sin(np.pi / 2 * t) ** 1.15  # builds toward the end, not the middle
    return band * env


def pop(rng: np.random.Generator) -> np.ndarray:
    n = int(0.12 * SR)
    tone = _glide_tone(n, 190, 70)
    env = env_pluck(n, SR, attack_s=0.001, decay_s=0.045)
    click_layer = lowpass(rng.standard_normal(n), 5200) * env_pluck(n, SR, 0.0005, 0.012) * 0.35
    return tone * env * 0.85 + click_layer


def tick(rng: np.random.Generator) -> np.ndarray:
    n = int(0.05 * SR)
    tone = np.sin(2 * np.pi * 2800 * np.arange(n) / SR)
    noise = lowpass(rng.standard_normal(n), 7000) * 0.3
    env = env_pluck(n, SR, attack_s=0.0005, decay_s=0.018)
    return (tone * 0.65 + noise * 0.35) * env


def click(rng: np.random.Generator) -> np.ndarray:
    n = int(0.022 * SR)
    noise = rng.standard_normal(n)
    band = lowpass(noise, 6500) - lowpass(noise, 1800)
    env = env_pluck(n, SR, attack_s=0.0003, decay_s=0.007)
    return band * env * 1.2


def chime(rng: np.random.Generator) -> np.ndarray:
    n = int(0.65 * SR)
    buf = np.zeros(n)
    note1 = bell(hz("1", 2), 0.55, amp=0.55)  # D6
    note2 = bell(hz("5", 2), 0.5, amp=0.4)  # A6, a fifth up - arrives just after
    buf[: len(note1)] += note1
    off = int(0.055 * SR)
    end = min(n, off + len(note2))
    buf[off:end] += note2[: end - off]
    return buf


def thump(rng: np.random.Generator) -> np.ndarray:
    n = int(0.24 * SR)
    tone = _glide_tone(n, 95, 52)
    env = env_pluck(n, SR, attack_s=0.006, decay_s=0.09)
    body = lowpass(rng.standard_normal(n), 200) * env * 0.25
    return tone * env * 0.9 + body


def shimmer(rng: np.random.Generator) -> np.ndarray:
    dur = 0.95
    n = int(dur * SR)
    buf = np.zeros(n)
    degrees = [("1", 3), ("3", 3), ("5", 3), ("6", 3), ("1", 4)]  # ascending sparkle, D major
    for i, (degree, octave) in enumerate(degrees):
        note = bell(hz(degree, octave), dur * (0.75 - 0.06 * i), amp=0.16)
        off = int(i * 0.05 * SR)
        end = min(n, off + len(note))
        buf[off:end] += note[: end - off]
    return buf


SOUNDS = {
    "whoosh-soft": whoosh_soft,
    "whoosh-fast": whoosh_fast,
    "swoosh-in": swoosh_in,
    "pop": pop,
    "tick": tick,
    "click": click,
    "chime": chime,
    "thump": thump,
    "shimmer": shimmer,
}


def render_all(names: list[str], seed: int = 20260927) -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    for name in names:
        stable_hash = int(hashlib.sha256(name.encode()).hexdigest()[:8], 16)
        rng = np.random.default_rng(seed + stable_hash % 1000)
        mono = _norm(SOUNDS[name](rng))
        out = OUT / f"{name}.wav"
        sf.write(out, _stereo(mono), SR, subtype="PCM_24")
        print(f"sfx: {out} ({len(mono) / SR * 1000:.0f} ms)")


if __name__ == "__main__":
    names = sys.argv[1:] or list(SOUNDS)
    unknown = [n for n in names if n not in SOUNDS]
    if unknown:
        raise SystemExit(f"sfx: unknown sound(s) {unknown}; choose from {list(SOUNDS)}")
    render_all(names)
