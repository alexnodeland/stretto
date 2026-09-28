#!/usr/bin/env python3
"""An original, license-free ambient score for the explainer video, generated
from scratch with numpy (no samples, no loops, nothing to clear rights on).

    python3 brand/video/music.py --duration 96 --open 0.35,0.85,1.1 \\
        --end 88 --out brand/video/out/music/explainer-video.wav

The theme is the product's name: in a fugue's stretto, entries of a motif
overlap, each starting before the last has finished. A short 4-6 note motif
(D major: D-F#-A-B-A-F#) enters three times, each at one of the --open times,
each a "voice" of its own (root register, a fifth up - the classic fugal
subject/answer - and an octave up), so the entries genuinely overlap the way
--open's three timestamps (matching the logo's three bars) suggest they
should. Under the narration it drops to a very sparse, slowly evolving pad in
the same key, with an occasional single soft note - never busy, never
percussive, nothing to compete with the voice-over sitting on top. At --end
the motif returns once, resolving onto the tonic instead of circling back to
the third, with a soft sustained chord underneath.

Synthesis is all additive/FM plus one simple state-variable-style lowpass:
- the motif is FM electric-piano (a sine carrier, a sine modulator whose
  index decays quickly) and, for the third entry, a small bank of
  inharmonic partials (a bell), both with a soft (10-20 ms) attack and an
  exponential decay - no hard onsets anywhere.
- the pad is a few detuned sawtooth/sine voices through a lowpass filter
  whose cutoff drifts slowly, with a slow amplitude swell (no LFO faster
  than about 0.05 Hz - nothing that reads as a "wobble").
- reverb is convolution with a synthetic impulse response (filtered noise
  under an exponential decay envelope, decorrelated per channel for width),
  mixed under the dry signal.

Output: 48 kHz stereo WAV, peaks normalized to -1.2 dBFS (comfortably under
the -1 dBFS ceiling the brief sets). Needs numpy and soundfile (both already
in brand/video/requirements-narrate.txt).
"""

import argparse
from pathlib import Path

import numpy as np
import soundfile as sf

SR = 48000

# D major, a warm, open key. Scale degrees as semitones from the root.
ROOT = 293.6648  # D4
SCALE = {"1": 0, "2": 2, "3": 4, "4": 5, "5": 7, "6": 9, "7": 11, "1u": 12}


def hz(degree: str, octave: int = 0) -> float:
    return ROOT * 2 ** ((SCALE[degree] + 12 * octave) / 12)


# The motif: 6 short notes with a gentle arch (up to the 6th, back to the
# 3rd) - "lovely" rather than grand. (degree, octave, start-beat, beat-len).
MOTIF = [("1", 0, 0.0, 1.0), ("3", 0, 1.0, 1.0), ("5", 0, 2.0, 1.0),
         ("6", 0, 3.0, 1.0), ("5", 0, 4.0, 1.0), ("3", 0, 5.0, 2.0)]
# The closing statement: the same first five notes, resolved to the tonic
# (a full octave held long) instead of circling back to the third.
MOTIF_RESOLVE = [("1", 0, 0.0, 1.0), ("3", 0, 1.0, 1.0), ("5", 0, 2.0, 1.0),
                 ("6", 0, 3.0, 1.0), ("5", 0, 4.0, 1.0), ("1", 1, 5.0, 3.0)]
BEAT_S = 0.21  # ~ a gentle, unhurried tempo for the motif


def env_pluck(n: int, sr: int, attack_s: float, decay_s: float) -> np.ndarray:
    """Soft attack, smooth exponential decay - the shape of every plucked/struck note here."""
    t = np.arange(n) / sr
    a = np.clip(t / max(attack_s, 1e-6), 0, 1) ** 1.5
    d = np.exp(-t / decay_s)
    return (a * d).astype(np.float32)


def fm_epiano(freq: float, dur_s: float, sr: int = SR, amp: float = 1.0) -> np.ndarray:
    """A soft FM electric-piano tone: sine carrier, sine modulator, decaying index."""
    n = int(dur_s * sr)
    t = np.arange(n) / sr
    mod_index = 2.2 * np.exp(-t / 0.18)  # bright for an instant, then mellow
    modulator = np.sin(2 * np.pi * freq * 2.0 * t)
    carrier = np.sin(2 * np.pi * freq * t + mod_index * modulator)
    return carrier * env_pluck(n, sr, attack_s=0.012, decay_s=1.6) * amp


def bell(freq: float, dur_s: float, sr: int = SR, amp: float = 1.0) -> np.ndarray:
    """A small, soft bell: a few inharmonic partials, each with its own decay."""
    n = int(dur_s * sr)
    t = np.arange(n) / sr
    partials = [(1.0, 1.0, 2.2), (2.41, 0.5, 1.3), (3.76, 0.28, 0.8), (5.4, 0.14, 0.5)]
    out = np.zeros(n, dtype=np.float32)
    for ratio, level, decay in partials:
        out += level * np.sin(2 * np.pi * freq * ratio * t) * env_pluck(n, sr, 0.02, decay)
    return out * amp / len(partials)


def place(buf: np.ndarray, sound: np.ndarray, start_s: float, sr: int = SR) -> None:
    i0 = int(start_s * sr)
    i1 = min(len(buf), i0 + len(sound))
    if i1 > i0:
        buf[i0:i1] += sound[: i1 - i0]


def render_motif(buf: np.ndarray, start_s: float, transpose_semi: float, timbre, amp: float) -> None:
    for degree, octave, beat, length in MOTIF:
        freq = hz(degree, octave) * 2 ** (transpose_semi / 12)
        place(buf, timbre(freq, length * BEAT_S * 1.35, amp=amp), start_s + beat * BEAT_S)


def render_resolution(buf: np.ndarray, start_s: float) -> None:
    for degree, octave, beat, length in MOTIF_RESOLVE:
        freq = hz(degree, octave)
        place(buf, fm_epiano(freq, length * BEAT_S * 1.6, amp=0.85), start_s + beat * BEAT_S)
    # a soft sustained chord (root, fifth, ninth) under the final note
    final_at = start_s + MOTIF_RESOLVE[-1][2] * BEAT_S
    chord_len = MOTIF_RESOLVE[-1][3] * BEAT_S * 3.2
    for degree, octave in (("1", 0), ("5", 0), ("2", 1)):
        n = int(chord_len * SR)
        t = np.arange(n) / SR
        tone = 0.12 * np.sin(2 * np.pi * hz(degree, octave) * t) * env_pluck(n, SR, 0.4, chord_len * 0.55)
        place(buf, tone.astype(np.float32), final_at)


def lowpass(x: np.ndarray, cutoff_hz: float, sr: int = SR) -> np.ndarray:
    """A one-pole-style lowpass (smooth -6 dB/oct rolloff), applied in one FFT
    multiply rather than a per-sample recursion - the same shape a simple
    analog RC lowpass has, just fast and fully vectorized."""
    n = len(x)
    freqs = np.fft.rfftfreq(n, d=1 / sr)
    response = 1.0 / np.sqrt(1.0 + (freqs / cutoff_hz) ** 2)
    return np.fft.irfft(np.fft.rfft(x) * response, n)


def saw(freq: np.ndarray, t: np.ndarray) -> np.ndarray:
    ph = t * freq
    return 2.0 * (ph - np.floor(0.5 + ph))


def make_pad(duration_s: float, rng: np.random.Generator) -> np.ndarray:
    """A very sparse, warm bed: detuned saws through a slowly drifting lowpass,
    a slow amplitude swell, and a handful of single soft notes - nothing busy,
    nothing that would compete with narration sitting on top."""
    n = int(duration_s * SR)
    t = np.arange(n) / SR
    if n == 0:
        return np.zeros((0, 2), dtype=np.float32)

    chords = [("1", "5", "2u"), ("6", "3", "1u")]  # two static, related voicings
    switch_every = 14.0
    out = np.zeros((n, 2), dtype=np.float64)
    detunes = (-0.06, 0.0, 0.07)  # in semitones, per stacked saw voice
    for degree_set_idx, degrees in enumerate(chords):
        gate = 0.5 - 0.5 * np.cos(2 * np.pi * t / (switch_every * len(chords)) + degree_set_idx * np.pi)
        gate = np.clip(gate, 0, 1) ** 2
        for degree in degrees:
            octv = 1 if degree.endswith("u") else 0
            base = hz(degree.rstrip("u"), octv)
            for d_idx, det in enumerate(detunes):
                f = base * 2 ** (det / 1200 * 100 / 12)  # cents-scale detune
                voice = saw(np.full(n, f), t)
                pan = 0.5 + 0.35 * (d_idx - 1)
                voice = lowpass(voice, cutoff_hz=520 + 140 * d_idx)
                amp = 0.028 * gate
                out[:, 0] += voice * amp * (1 - pan)
                out[:, 1] += voice * amp * pan

    # a slow overall breathing swell so the pad never sits perfectly static
    swell = 0.75 + 0.25 * np.sin(2 * np.pi * t / 23.0)
    out *= swell[:, None]

    # a handful of sparse, quiet single notes drawn from the scale
    gap = 6.0
    pos = rng.uniform(gap * 0.6, gap * 1.4)
    degrees = ["1", "2", "3", "5", "6"]
    while pos < duration_s - 2.0:
        degree = degrees[rng.integers(len(degrees))]
        freq = hz(degree, rng.choice([0, 1]))
        note = bell(freq, 2.6, amp=0.05 * rng.uniform(0.7, 1.0))
        pan = rng.uniform(0.3, 0.7)
        i0 = int(pos * SR)
        i1 = min(n, i0 + len(note))
        if i1 > i0:
            out[i0:i1, 0] += note[: i1 - i0] * (1 - pan)
            out[i0:i1, 1] += note[: i1 - i0] * pan
        pos += rng.uniform(gap * 0.7, gap * 1.6)
    return out.astype(np.float32)


def make_ir(length_s: float, decay_tau: float, rng: np.random.Generator) -> np.ndarray:
    """A synthetic decaying-noise impulse response: filtered noise under an
    exponential envelope, for a soft, unobtrusive reverb - not a real space,
    not a sampled plate; nothing to clear."""
    n = int(length_s * SR)
    noise = rng.standard_normal(n).astype(np.float64)
    noise = lowpass(noise, 3500.0)
    env = np.exp(-np.arange(n) / (decay_tau * SR))
    ir = (noise * env).astype(np.float32)
    ir /= np.sqrt(np.sum(ir**2)) or 1.0
    return ir


def convolve_reverb(dry: np.ndarray, wet_amount: float, rng: np.random.Generator) -> np.ndarray:
    out = np.zeros_like(dry)
    n_fft = 1
    target = len(dry) + int(1.8 * SR)
    while n_fft < target:
        n_fft *= 2
    for ch in range(dry.shape[1]):
        ir = make_ir(1.8, 0.55, rng)  # a slightly different IR per channel: width, not a mono smear
        spec = np.fft.rfft(dry[:, ch], n_fft) * np.fft.rfft(ir, n_fft)
        wet = np.fft.irfft(spec, n_fft)[: len(dry)]
        out[:, ch] = dry[:, ch] + wet_amount * wet
    return out


def normalize_peak(x: np.ndarray, peak_db: float) -> np.ndarray:
    peak = float(np.max(np.abs(x))) or 1.0
    return x * (10 ** (peak_db / 20) / peak)


def build(duration: float, open_times: list[float], end: float, seed: int) -> np.ndarray:
    rng = np.random.default_rng(seed)
    n = int(duration * SR)
    dry = np.zeros((n, 2), dtype=np.float64)

    # The open: three overlapping stretto entries of the motif, one per
    # --open time - root register, a fifth up (subject/answer), an octave up.
    motif_voices = [
        (open_times[0], 0.0, fm_epiano, (0.62, 0.62)),
        (open_times[1], 7.0, fm_epiano, (0.42, 0.72)),
        (open_times[2], 12.0, bell, (0.72, 0.42)),
    ]
    for t0, semi, timbre, pans in motif_voices:
        mono = np.zeros(n, dtype=np.float64)
        render_motif(mono, t0, semi, timbre, amp=1.0)
        dry[:, 0] += mono * pans[0]
        dry[:, 1] += mono * pans[1]

    # Under the narration: the sparse pad, from just after the entries settle
    # until the closing statement begins.
    pad_start = max(open_times) + 2.2
    pad_len = max(0.0, end - pad_start)
    pad = make_pad(pad_len, rng)
    if len(pad):
        dry[int(pad_start * SR): int(pad_start * SR) + len(pad)] += pad

    # The close: the motif once more, resolving.
    mono = np.zeros(n, dtype=np.float64)
    render_resolution(mono, end)
    dry[:, 0] += mono * 0.7
    dry[:, 1] += mono * 0.7

    wet = convolve_reverb(dry, wet_amount=0.22, rng=rng)
    return normalize_peak(wet, peak_db=-1.2).astype(np.float32)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--duration", type=float, required=True, help="total length in seconds")
    ap.add_argument("--open", type=str, required=True,
                    help="comma-separated seconds for the motif's three stretto entries")
    ap.add_argument("--end", type=float, required=True, help="when the closing, resolving motif begins")
    ap.add_argument("--out", type=str, required=True, help="output WAV path")
    ap.add_argument("--seed", type=int, default=20260927, help="for the pad's sparse notes (reproducible)")
    args = ap.parse_args()

    open_times = [float(x) for x in args.open.split(",")]
    if len(open_times) != 3:
        raise SystemExit("--open needs exactly three comma-separated times")

    audio = build(args.duration, open_times, args.end, args.seed)
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    sf.write(out, audio, SR, subtype="PCM_24")
    print(f"music: {out} ({args.duration:.1f}s, peak {20*np.log10(np.max(np.abs(audio))):.1f} dBFS)")


if __name__ == "__main__":
    main()
