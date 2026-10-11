"""Chiptune-flavoured sound design -> mono Ogg Vorbis. Python stdlib plus
`mb ogg` (or oggenc/oggdec from vorbis-tools, with an older mb).

Effects:

    import sys; sys.path.insert(0, "<skill dir>/scripts")   # or copy this file into your tools/
    from sfx import Sfx, Song, Inst, mix, noise, reverb, echo, save_ogg, save_stems

    # An instrument (preset name or Inst) playing a possibly sweeping note.
    jump = Sfx("square", freq=330, freq_end=880, dur=0.18)
    coin = Sfx("pluck", freq=988, dur=0.25, arp=(0, 5))        # two-note arpeggio
    boom = mix(Sfx("kick", freq=60, dur=0.5).render(), Sfx("noise_burst", freq=900, dur=0.6).render())
    save_ogg("games/g/assets/jump.ogg", jump.render())
    save_ogg("games/g/assets/boom.ogg", reverb(boom, room=0.6, wet=0.25))
    # Layered in time: offsets are SECONDS from the start (not samples).
    ding_dong = mix(Sfx("bell", 784, dur=0.4).render(), Sfx("bell", 659, dur=0.6).render(), offsets=[0.0, 0.25])
    # Any preset setting can be overridden, or an instrument built whole; the
    # envelope is one tuple, adsr=(attack s, decay s, sustain 0–1, release s).
    soft = Inst("triangle", adsr=(0.02, 0.2, 0.5, 0.3), cutoff=3000, vibrato=(0.3, 6.0, 0.1))
    hum = Sfx(soft, freq=220, dur=0.8)
    # Smooth filtered noise with an attack and an exponential decay: a hiss,
    # a "shh", wind, a whoosh (the noise presets are grittier, 8-bit noise).
    shh = noise(0.6, cutoff=5000, attack=0.08, decay=0.35)

Music (a step sequencer with instruments, accents, seamless loops and stems):

    song = Song(bpm=128, swing=0.1)
    song.track("lead",  "E5 . G5 . A5 - - . G5 . E5 . D5 . . .", name="lead")
    song.track("bass",  "A2 . A2 . A2 . G2 . F2 . F2 . G2 . G2 .", name="bass")
    song.track("kick",  "x . . . x . . . x . . . x . x .", name="drums")
    song.track("snare", ". . . . x . . . . . . . x . . X", name="drums")
    song.track("hat",   "x x x x x x x x x x x x x x x X", name="drums", vol=0.5)
    song.track("arp",   "A4 C5 E5 A5 A4 C5 E5 A5 G4 B4 D5 G5 F4 A4 C5 F5", name="tension")
    stems = song.stems(bars=4, groups={"base": ["bass", "drums", "lead"], "tension": ["tension"]})
    stems["base"] = reverb(stems["base"], 0.3, 0.12, loop=True)
    save_stems(stems, {"base": "games/g/assets/music.ogg", "tension": "games/g/assets/music_hi.ogg"})
    # In the game: start both looped in the same frame (they stay in sync),
    # the tension layer at vol 0, and raise its vol as the danger rises.

Patterns: space-separated steps, one per 16th note by default:
  a note `C5` `F#3` `Bb2`;  `.` rest;  `-` hold the previous note;
  `x` hit (drums/noise);  a trailing `!` (or capital `X`) accents, `?` plays softly.
  A pattern shorter than the song repeats.

Presets (Inst names): square, pulse, triangle, saw, sine, lead, pluck, bass,
pad, arp, bell, kick, snare, hat, clap, tom, noise_burst, zap.

Levels: save_ogg normalizes by loudness, not peak: kind="sfx" (default)
targets about -14 dBFS RMS, kind="music" about -20, so music sits under the
effects at equal `vol`. Mix in the game with vol around 0.6–1.0. Peaks are
held below -3 dBFS by a look-ahead limiter, and the encoded file is decoded
and re-encoded quieter if Vorbis pushed a peak past -1 dBFS, so anything
saved passes check_audio's clip check. Very peaky sounds (a sharp hit with a
long reverb tail) therefore come out below the loudness target; `drive()`
before saving makes them denser and louder if they need it.

Foley packs: `python3 sfx.py --pack mech games/g/assets [prefix]` writes a
mech's stomp, servo (loop), thruster (loop), hiss, impact and tear as
`mech_<sound>.ogg` (or `<prefix><sound>.ogg`), for `maimbrain::foley`.

Self-test: python3 sfx.py --self-test
"""

from __future__ import annotations

import math
import os
import random
import shutil
import struct
import subprocess
import sys
import tempfile
import wave

RATE = 44100
TAU = 2 * math.pi
_NOTE = {"C": -9, "D": -7, "E": -5, "F": -4, "G": -2, "A": 0, "B": 2}


def note_freq(name: str) -> float:
    """'A4' = 440, 'C#5', 'Eb3'."""
    n, rest = name[0].upper(), name[1:]
    semis = _NOTE[n]
    while rest and rest[0] in "#b":
        semis += 1 if rest[0] == "#" else -1
        rest = rest[1:]
    return 440.0 * 2 ** ((semis + (int(rest) - 4) * 12) / 12)


# --- instruments ---------------------------------------------------------------

class Inst:
    """A synth voice.

    wave: square | triangle | saw | sine | noise (+ wave2 for a second oscillator)
    duty / pwm: pulse width and its modulation (rate Hz, depth)
    detune: cents between the two oscillators (fattens leads and pads)
    adsr: attack, decay, sustain level, release (seconds / 0–1)
    cutoff, res, env_cutoff: a resonant low-pass; env_cutoff Hz added at the
        note's start and decaying over `filter_decay` (the classic "pluck/wah")
    pitch_env: (semitones, seconds): starts that far above and falls to pitch
        (drums, zaps, lasers)
    vibrato: (semitones, rate Hz, delay s)
    noise: 0–1 noise mixed into the tone (noise_hz sets its brightness)
    crush: bit depth for 8-bit grit (0 = off)
    """

    def __init__(self, wave="square", wave2=None, duty=0.5, pwm=(0.0, 0.0), detune=0.0,
                 adsr=(0.005, 0.1, 0.7, 0.08), cutoff=12000.0, res=0.1, env_cutoff=0.0, filter_decay=0.15,
                 pitch_env=(0.0, 0.0), vibrato=(0.0, 5.5, 0.15), noise=0.0, crush=0, gain=1.0, noise_hz=None):
        for w in (wave, wave2):
            if w is not None and w not in _WAVES:
                hint = f" ({w!r} is a preset: use Sfx({w!r}, ...) or inst({w!r}))" if w in PRESETS else ""
                raise ValueError(f"unknown wave {w!r}; waves are {sorted(_WAVES)}{hint}")
        self.__dict__.update({k: v for k, v in locals().items() if k not in ("self", "w", "hint")})

    def render(self, freq: float, dur: float, vel: float = 1.0, freq_end: float | None = None,
               arp=(), arp_step=0.06, seed: int = 1) -> list[float]:
        a, d, s, r = self.adsr
        n = int((dur + r) * RATE)
        out = [0.0] * n
        ph1 = ph2 = 0.0
        lfsr, noise_v, noise_ph = (seed * 7919 & 0x7FFF) or 1, 0.0, 0.0
        low = band = 0.0
        q = 1.0 - min(0.95, self.res)
        semis0, pt = self.pitch_env
        vs, vr, vd = self.vibrato
        f_end = freq_end or freq
        for i in range(n):
            t = i / RATE
            # pitch
            f = freq * (f_end / freq) ** min(1.0, t / dur) if f_end != freq else freq
            if semis0 and pt:
                f *= 2 ** (semis0 * max(0.0, 1 - t / pt) ** 2 / 12)
            if vs and t > vd:
                f *= 2 ** (vs * math.sin(TAU * vr * (t - vd)) / 12)
            if arp:
                f *= 2 ** (arp[int(t / arp_step) % len(arp)] / 12)
            # oscillators
            ph1 = (ph1 + f / RATE) % 1.0
            duty = self.duty + self.pwm[1] * math.sin(TAU * self.pwm[0] * t)
            v = _wave(self.wave, ph1, duty)
            if self.wave2:
                ph2 = (ph2 + f * 2 ** (self.detune / 1200) / RATE) % 1.0
                v = 0.5 * (v + _wave(self.wave2, ph2, duty))
            if self.wave == "noise" or self.noise:
                noise_ph += (self.noise_hz or max(f * 8, 2000)) / RATE
                while noise_ph >= 1:
                    noise_ph -= 1
                    bit = (lfsr ^ (lfsr >> 1)) & 1
                    lfsr = (lfsr >> 1) | (bit << 14)
                    noise_v = 1.0 if lfsr & 1 else -1.0
                v = noise_v if self.wave == "noise" else v * (1 - self.noise) + noise_v * self.noise
            # filter (Chamberlin state-variable low-pass)
            fc = min(9000.0, self.cutoff + self.env_cutoff * math.exp(-t / max(1e-3, self.filter_decay)))
            fq = 2 * math.sin(math.pi * fc / RATE)
            low += fq * band
            high = v - low - q * band
            band += fq * high
            v = low
            # envelope
            if t < a:
                env = t / a
            elif t < a + d:
                env = 1 - (1 - s) * (t - a) / d
            else:
                env = s
            if t > dur:
                env *= max(0.0, 1 - (t - dur) / r) if r else 0.0
            v *= env * vel * self.gain
            if self.crush:
                steps = 2 ** (self.crush - 1)
                v = round(v * steps) / steps
            out[i] = v
        return out


_WAVES = {"square", "triangle", "saw", "sine", "noise"}


def _wave(w: str, ph: float, duty: float) -> float:
    if w == "square":
        return 1.0 if ph < duty else -1.0
    if w == "triangle":
        return 4 * abs(ph - 0.5) - 1
    if w == "saw":
        return 2 * ph - 1
    if w == "sine":
        return math.sin(TAU * ph)
    return 0.0  # noise is handled by the caller


PRESETS: dict[str, Inst] = {}  # filled below; Inst() checks names against it
PRESETS.update({
    "square": Inst("square", duty=0.5),
    "pulse": Inst("square", duty=0.25),
    "triangle": Inst("triangle", adsr=(0.003, 0.05, 0.9, 0.05)),
    "saw": Inst("saw", cutoff=5000),
    "sine": Inst("sine"),
    "lead": Inst("square", wave2="square", duty=0.25, detune=9, pwm=(0.7, 0.12), adsr=(0.005, 0.12, 0.65, 0.12),
                 cutoff=3800, res=0.25, env_cutoff=2500, vibrato=(0.18, 5.5, 0.18)),
    "pluck": Inst("square", duty=0.25, adsr=(0.002, 0.18, 0.0, 0.08), cutoff=1800, env_cutoff=5000, filter_decay=0.08, res=0.3),
    "bass": Inst("triangle", wave2="square", duty=0.5, detune=-6, adsr=(0.004, 0.1, 0.8, 0.05), cutoff=900, env_cutoff=900,
                 filter_decay=0.1, res=0.3, gain=1.1),
    "pad": Inst("saw", wave2="saw", detune=14, adsr=(0.25, 0.4, 0.75, 0.5), cutoff=1600, res=0.15, vibrato=(0.08, 4.0, 0.3), gain=0.7),
    "arp": Inst("square", duty=0.125, adsr=(0.002, 0.07, 0.25, 0.05), cutoff=4200, env_cutoff=3000, filter_decay=0.05),
    "bell": Inst("sine", wave2="triangle", detune=1200 * 1.5, adsr=(0.002, 0.6, 0.0, 0.3), gain=0.8),
    "kick": Inst("sine", adsr=(0.001, 0.22, 0.0, 0.05), pitch_env=(30, 0.12), cutoff=3000, gain=1.4),
    "snare": Inst("triangle", noise=0.75, noise_hz=9000, adsr=(0.001, 0.14, 0.0, 0.06), pitch_env=(12, 0.05), cutoff=7000),
    "hat": Inst("noise", noise_hz=16000, adsr=(0.001, 0.035, 0.0, 0.02), cutoff=9000, gain=0.5),
    "clap": Inst("noise", noise_hz=7000, adsr=(0.004, 0.12, 0.0, 0.05), cutoff=4000, res=0.4, gain=0.9),
    "tom": Inst("triangle", adsr=(0.001, 0.25, 0.0, 0.05), pitch_env=(14, 0.15), gain=1.2),
    "noise_burst": Inst("noise", noise_hz=6000, adsr=(0.002, 0.3, 0.0, 0.2), cutoff=2500, env_cutoff=5000, filter_decay=0.2),
    "zap": Inst("saw", adsr=(0.001, 0.15, 0.0, 0.05), pitch_env=(24, 0.15), cutoff=6000, res=0.4),
})

# Drum presets play at a fixed, sensible pitch when triggered with `x`.
_DRUM_HZ = {"kick": 50.0, "snare": 190.0, "hat": 8000.0, "clap": 1200.0, "tom": 110.0, "noise_burst": 800.0}


def inst(i: str | Inst) -> Inst:
    return PRESETS[i] if isinstance(i, str) else i


class Sfx:
    """One effect: an instrument playing a (possibly sweeping) note.

    freq -> freq_end over dur; arp cycles semitone offsets every arp_step;
    repeat/gap make stutters (glug-glug-glug), each repeat shifted by
    pitch_step semitones. Extra keyword arguments override the instrument's
    settings, e.g. Sfx("pluck", 660, dur=0.2, cutoff=900)."""

    def __init__(self, instrument: str | Inst = "square", freq=440.0, freq_end=None, dur=0.2, vel=1.0,
                 arp=(), arp_step=0.06, repeat=1, gap=0.04, pitch_step=0.0, **overrides):
        base = inst(instrument)
        self.inst = Inst(**{**base.__dict__, **overrides}) if overrides else base
        self.freq, self.freq_end, self.dur, self.vel = freq, freq_end, dur, vel
        self.arp, self.arp_step, self.repeat, self.gap, self.pitch_step = arp, arp_step, repeat, gap, pitch_step

    def render(self) -> list[float]:
        out: list[float] = []
        for k in range(self.repeat):
            m = 2 ** (k * self.pitch_step / 12)
            fe = self.freq_end * m if self.freq_end else None
            part = self.inst.render(self.freq * m, self.dur, self.vel, fe, self.arp, self.arp_step, seed=k + 1)
            start = int(k * (self.dur + self.gap) * RATE)
            if len(out) < start + len(part):
                out.extend([0.0] * (start + len(part) - len(out)))
            for i, v in enumerate(part):
                out[start + i] += v
        return out


# --- sequencing ---------------------------------------------------------------

class Song:
    """A step sequencer. Each track is an instrument and a pattern."""

    def __init__(self, bpm=120, steps_per_beat=4, swing=0.0):
        self.bpm, self.steps_per_beat, self.swing = bpm, steps_per_beat, swing
        self.step_len = 60.0 / bpm / steps_per_beat
        self.tracks: list[dict] = []

    def track(self, instrument: str | Inst, pattern: str, vol=0.5, name: str | None = None, octave=0, note_dur=None):
        """vol: this track's level in the mix; octave: shift by octaves;
        note_dur: fixed note length in seconds (else until the next step)."""
        self.tracks.append({"inst": instrument, "pat": pattern.split(), "vol": vol, "octave": octave,
                            "name": name or (instrument if isinstance(instrument, str) else "track"), "note_dur": note_dur})

    def _render_track(self, t: dict, steps: int, n: int) -> list[float]:
        out = [0.0] * n
        pat, ins = t["pat"], inst(t["inst"])
        drum = isinstance(t["inst"], str) and t["inst"] in _DRUM_HZ
        for s in range(steps):
            tok = pat[s % len(pat)]
            if tok in (".", "-"):
                continue
            vel = 1.0
            if tok.endswith("!") or tok == "X":
                vel, tok = 1.35, tok.rstrip("!")
            elif tok.endswith("?"):
                vel, tok = 0.55, tok.rstrip("?")
            if tok.lower() == "x":
                freq = _DRUM_HZ.get(t["inst"], 440.0) if drum else 440.0
            else:
                freq = note_freq(tok) * 2 ** t["octave"]
            j = s + 1
            while j < steps and pat[j % len(pat)] == "-":
                j += 1
            length = t["note_dur"] or (j - s) * self.step_len * 0.92
            start = s * self.step_len + (self.swing * self.step_len if s % 2 else 0.0)
            part = ins.render(freq, length, vel * t["vol"], seed=s + 1)
            a = int(start * RATE)
            for i, v in enumerate(part):
                if a + i < len(out):
                    out[a + i] += v
        return out

    def render(self, bars=4, beats_per_bar=4, names: list[str] | None = None, loop=True, tail=1.5) -> list[float]:
        """Mix of the named tracks (all by default). loop=True renders a tail
        (note releases) and folds it onto the start, so the file loops
        seamlessly at exactly `bars` bars."""
        steps = bars * beats_per_bar * self.steps_per_beat
        length = int(round(steps * self.step_len * RATE))
        n = length + int(tail * RATE)
        mixdown = [0.0] * n
        for t in self.tracks:
            if names is None or t["name"] in names:
                for i, v in enumerate(self._render_track(t, steps, n)):
                    mixdown[i] += v
        if not loop:
            return mixdown
        out = mixdown[:length]
        for i in range(length, n):
            out[(i - length) % length] += mixdown[i]
        return out

    def stems(self, bars=4, groups: dict[str, list[str]] | None = None, beats_per_bar=4) -> dict[str, list[float]]:
        """Separately rendered, identically long loops (one per group of
        track names), for layering music by intensity in the game."""
        groups = groups or {t["name"]: [t["name"]] for t in self.tracks}
        return {g: self.render(bars, beats_per_bar, names) for g, names in groups.items()}


# --- effects ------------------------------------------------------------------

def mix(*parts, offsets=None, gains=None) -> list[float]:
    """Layers sounds (e.g. a thump under a sweep). `offsets`: when each part
    starts, in seconds (not samples), one per part; `gains`: one per part."""
    offsets = offsets or [0.0] * len(parts)
    gains = gains or [1.0] * len(parts)
    if len(offsets) != len(parts) or len(gains) != len(parts):
        raise ValueError(f"mix: {len(parts)} parts, {len(offsets)} offsets and {len(gains)} gains; give one of each per part")
    for o in offsets:
        # An offset in samples (0.09 * RATE) would pad hours of silence and
        # run for many minutes: refuse it.
        if not 0 <= o <= 60:
            hint = f" ({o:g} samples would be {o / RATE:.3f} s)" if o > 60 else ""
            raise ValueError(f"mix: offsets are seconds from the start, 0-60; got {o:g}{hint}")
    starts = [int(o * RATE) for o in offsets]
    out = [0.0] * max(s + len(p) for s, p in zip(starts, parts))
    for s, p, g in zip(starts, parts, gains):
        for i, v in enumerate(p):
            out[s + i] += v * g
    return out


def echo(x: list[float], time=0.18, feedback=0.35, wet=0.3, loop=False) -> list[float]:
    """A feedback delay. loop=True wraps the echoes around (for loops)."""
    d = max(1, int(time * RATE))
    n = len(x) if loop else len(x) + d * 6
    dry = list(x) + [0.0] * (n - len(x))
    y = [0.0] * n
    for _ in range(2 if loop else 1):
        for i in range(n):
            j = i - d
            if j < 0 and not loop:
                continue
            y[i] = dry[j % n] + feedback * y[j % n]
    return [a + wet * b for a, b in zip(dry, y)]


def reverb(x: list[float], room=0.5, wet=0.2, damp=0.4, loop=False) -> list[float]:
    """A small Schroeder reverb (4 combs + 2 all-passes); room 0–1 sets the
    decay. Adds space to effects; keep wet ≤ 0.3 so things stay punchy.
    loop=True wraps the tail around for seamless music loops."""
    fb = 0.7 + 0.28 * room
    tail = 0 if loop else int(RATE * (0.5 + 2.5 * room))
    n = len(x) + tail
    src = list(x) + [0.0] * tail
    acc = [0.0] * n
    passes = 2 if loop else 1  # a second pass lets the wrapped tail settle
    for c in (1557, 1617, 1491, 1422):
        buf, store, idx = [0.0] * c, 0.0, 0
        for p in range(passes):
            for i in range(n):
                y = buf[idx]
                store = y * (1 - damp) + store * damp
                buf[idx] = src[i] + store * fb
                idx = (idx + 1) % c
                if p == passes - 1:
                    acc[i] += y
    for c, g in ((225, 0.5), (556, 0.5)):
        buf, idx, res = [0.0] * c, 0, [0.0] * n
        for _ in range(passes):  # all-passes carry state across the seam too
            for i in range(n):
                y = buf[idx]
                buf[idx] = acc[i] + y * g
                res[i] = y - acc[i] * g
                idx = (idx + 1) % c
        acc = res
    return [s * (1 - wet) + a * 0.25 * wet for s, a in zip(src, acc)]


def lowpass(x: list[float], cutoff=2000.0) -> list[float]:
    k = 1 - math.exp(-TAU * cutoff / RATE)
    y, out = 0.0, []
    for v in x:
        y += (v - y) * k
        out.append(y)
    return out


def drive(x: list[float], amount=2.0) -> list[float]:
    """Soft saturation: warmth and grit, and tames peaks."""
    return [math.tanh(v * amount) / math.tanh(amount) for v in x]


def vary(sfx: Sfx, n=3, cents=40, seed=7) -> list[list[float]]:
    """n slightly detuned renders of one effect (load them as separate sounds
    and pick one at random, so repeated sounds don't machine-gun). Often it's
    enough to vary the `pitch` argument of audio::play by ±3% instead."""
    rng = random.Random(seed)
    outs = []
    for _ in range(n):
        k = 2 ** (rng.uniform(-cents, cents) / 1200)
        s = Sfx(sfx.inst, sfx.freq * k, sfx.freq_end * k if sfx.freq_end else None, sfx.dur, sfx.vel,
                sfx.arp, sfx.arp_step, sfx.repeat, sfx.gap, sfx.pitch_step)
        outs.append(s.render())
    return outs


# --- output -------------------------------------------------------------------

def _gain(x: list[float], kind: str) -> float:
    """Gain that brings the RMS of the loud part of x to the kind's target."""
    target = {"sfx": -14.0, "music": -20.0}[kind]
    sq = sorted(v * v for v in x)
    top = sq[len(sq) // 3:] or sq  # ignore near-silence
    rms = math.sqrt(sum(top) / len(top)) if top else 0.0
    return 10 ** (target / 20) / (rms or 1e-9)


# Sample peaks are limited to CEILING_DB before encoding; after encoding the
# file is decoded and, if Vorbis pushed a peak above DECODED_MAX_DB, scaled
# down and encoded again. check_audio.py flags anything above -0.5 dBFS.
CEILING_DB = -3.0
DECODED_MAX_DB = -1.0


def _limit(x: list[float], g: float) -> list[float]:
    """Applies gain g, then a look-ahead peak limiter: a smooth gain envelope
    that is already down when a peak arrives and recovers over ~80 ms, so no
    sample exceeds CEILING_DB. Unlike clipping or waveshaping it adds no
    harmonics, which is what made peaky sounds (explosions with long tails,
    where loudness normalization asks for a lot of gain) overshoot after
    Vorbis encoding."""
    from collections import deque
    ceiling = 10 ** (CEILING_DB / 20)
    y = [v * g for v in x]
    n = len(y)
    need = [ceiling / abs(v) if abs(v) > ceiling else 1.0 for v in y]
    la = max(1, int(0.0015 * RATE))
    # m[i] = min(need[i-la .. i+la]) (sliding-window minimum)
    m, q = [1.0] * n, deque()
    for j in range(n + la):
        if j < n:
            while q and need[q[-1]] >= need[j]:
                q.pop()
            q.append(j)
        i = j - la
        if i >= 0:
            while q[0] < i - la:
                q.popleft()
            m[i] = need[q[0]]
    # a[i] = mean(m[i-la .. i+la]): every m in that window is <= need[i], so a[i] <= need[i].
    acc = [0.0]
    for v in m:
        acc.append(acc[-1] + v)
    k = 1 - math.exp(-1 / (0.08 * RATE))
    out, env = [], 1.0
    for i in range(n):
        lo, hi = max(0, i - la), min(n, i + la + 1)
        a = (acc[hi] - acc[lo]) / (hi - lo)
        env = min(a, env + (1 - env) * k)
        out.append(y[i] * env)
    return out


def normalize(x: list[float], kind="sfx") -> list[float]:
    """Loudness-normalize: RMS of the loud part to ~-14 dBFS (sfx) or ~-20
    dBFS (music), with peaks limited below CEILING_DB (-3 dBFS)."""
    return _limit(x, _gain(x, kind)) if x else x


def save_wav(path: str, samples: list[float]) -> None:
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(b"".join(struct.pack("<h", int(max(-1, min(1, s)) * 32767)) for s in samples))


def save_stems(stems: dict[str, list[float]], paths: dict[str, str], loop: bool = True, quality: int = 5) -> None:
    """Saves music stems with ONE shared gain, set so all stems playing
    together hit the music target. (Normalizing each stem on its own would
    wreck the balance you wrote.) All stems must be the same length."""
    lengths = {len(v) for v in stems.values()}
    assert len(lengths) == 1, "stems must be the same length"
    total = [sum(vs) for vs in zip(*stems.values())]
    g = _gain(total, "music")
    for name, x in stems.items():
        _encode(paths[name], _limit(x, g), loop, quality)


def save_ogg(path: str, samples: list[float], loop: bool = False, kind: str = "sfx", quality: int = 5) -> None:
    """Normalizes (see `normalize`), writes a WAV and encodes it to Ogg.

    loop=True: for `play_looped`. Render loops with Song.render(loop=True)
    (and reverb/echo with loop=True) so the tail is already folded onto the
    start; a 1 ms fade at the ends hides any click if the decoder pads."""
    _encode(path, normalize(samples, kind), loop, quality)


def _encode(path: str, s: list[float], loop: bool, quality: int) -> None:
    if max((abs(v) for v in s), default=0.0) < 1e-4:
        raise ValueError(f"{path}: the sound is silent; check the instrument and notes")
    s = list(s)
    f = int(0.001 * RATE) if loop else int(0.004 * RATE)
    for i in range(min(f, len(s) // 2)):
        if loop:
            s[i] *= i / f
        s[-1 - i] *= i / f
    limit = 10 ** (DECODED_MAX_DB / 20)
    with tempfile.TemporaryDirectory() as d:
        w = os.path.join(d, "s.wav")
        for _ in range(4):
            save_wav(w, s)
            _ogg(w, path, quality)
            # Vorbis can overshoot the source's peaks: check what a player will decode.
            peak = _decoded_peak(path, d)
            if peak <= limit:
                return
            s = [v * limit / peak * 0.97 for v in s]
        raise RuntimeError(f"{path}: decoded peak still above {DECODED_MAX_DB} dBFS")


def _decoded_peak(path: str, tmp: str) -> float:
    """Peak of the decoded file."""
    out = os.path.join(tmp, "check.wav")
    _ogg(path, out)
    with wave.open(out) as r:
        n, ch = r.getnframes(), r.getnchannels()
        data = struct.unpack(f"<{n * ch}h", r.readframes(n))
    return max((abs(v) for v in data), default=0) / 32768


_MB = None  # the mb that has `mb ogg` ("" if there isn't one), found on first use


def _ogg(src: str, dst: str, quality: float = 5) -> None:
    """WAV -> mono Ogg Vorbis (dst ends in .ogg) or Ogg -> 16-bit WAV, with
    `mb ogg` (built into mb, on every OS) or else vorbis-tools' oggenc/oggdec."""
    global _MB
    if _MB is None:
        mb = shutil.which("mb")
        _MB = mb if mb and subprocess.run([mb, "ogg", "--help"], capture_output=True).returncode == 0 else ""
    enc = dst.lower().endswith(".ogg")
    if _MB:
        cmd = [_MB, "ogg", "-Q", "-q", str(quality), src, dst]
    elif shutil.which("oggenc" if enc else "oggdec"):
        cmd = ["oggenc", "-Q", "-q", str(quality), "-o", dst, src] if enc else ["oggdec", "-Q", "-o", dst, src]
    else:
        raise SystemExit("no Ogg Vorbis tool: update mb, which has `mb ogg` built in "
                         "(curl -fsSL https://maimbrain.com/install.sh | sh; on Windows irm https://maimbrain.com/install.ps1 | iex)")
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode:
        raise SystemExit(f"{' '.join(cmd)}: {r.stderr.strip()}")


# --- foley packs ----------------------------------------------------------------

def loopify(x: list[float], xfade=0.15) -> list[float]:
    """A seamless loop from a rendered stretch: its last `xfade` seconds fade
    (equal power) into its start, and are cut off, so the file wraps cleanly."""
    n = int(xfade * RATE)
    if len(x) < 3 * n:
        return list(x)
    body, tail = list(x[:-n]), x[-n:]
    for i in range(n):
        a = i / n
        body[i] = body[i] * math.sin(a * math.pi / 2) + tail[i] * math.cos(a * math.pi / 2)
    return body


def _env(n: int, attack: float, decay: float) -> list[float]:
    a = max(1, int(attack * RATE))
    return [min(1.0, i / a) * math.exp(-max(0, i - a) / (decay * RATE)) for i in range(n)]


def _partials(freqs, dur: float, decay: float, seed: int = 1) -> list[float]:
    """Inharmonic struck metal: sine partials, higher ones dying faster."""
    rng = random.Random(seed)
    n = int(dur * RATE)
    out = [0.0] * n
    for k, f in enumerate(freqs):
        ph, amp, dk = rng.random() * TAU, 1.0 / (1 + 0.6 * k), decay / (1 + 0.5 * k)
        for i in range(n):
            out[i] += amp * math.sin(ph + TAU * f * i / RATE) * math.exp(-i / (dk * RATE))
    return out


def _noise(dur: float, seed: int, cutoff: float) -> list[float]:
    rng = random.Random(seed)
    return lowpass([rng.uniform(-1, 1) for _ in range(int(dur * RATE))], cutoff)


def noise(dur: float, cutoff: float = 4000.0, attack: float = 0.005, decay: float = 0.2, seed: int = 1) -> list[float]:
    """White noise through a low-pass at `cutoff` Hz, shaped by an envelope:
    rising over `attack` seconds, then falling exponentially (to about a
    third every `decay` seconds) for `dur` seconds in all. A hiss, a "shh",
    a breath, wind (a long decay), a whoosh (layer two cutoffs), a crash's
    tail. Pass it to `mix`, `reverb` and `save_ogg` like any render."""
    x = _noise(dur, seed, cutoff)
    return [v * e for v, e in zip(x, _env(len(x), attack, decay))]


def mech_pack(seed: int = 3) -> dict[str, tuple[list[float], bool]]:
    """Foley for a mech (or any big machine), procedurally: name -> (samples,
    loops). Bright enough for a phone (each has a click or partials at
    1–4 kHz over its weight). Pairs with the SDK's `maimbrain::foley`: stomp
    for `Foley::step`/`impact`, servo and thruster for `Drive`, hiss, impact
    and tear for `Foley::hit`."""
    r = random.Random(seed)
    stomp = mix(
        Sfx("kick", 52, dur=0.32).render(),
        _partials([180, 410, 1250, 2300], 0.4, 0.06, seed),
        [v * e for v, e in zip(_noise(0.25, seed, 2400), _env(int(0.25 * RATE), 0.002, 0.04))],
        gains=[1.0, 0.45, 0.5],
    )
    t = [i / RATE for i in range(int(1.35 * RATE))]
    servo = [
        0.5 * math.sin(TAU * 330 * s + 0.6 * math.sin(TAU * 37 * s))
        + 0.3 * (((s * 495) % 1.0) * 2 - 1) * (0.6 + 0.4 * math.sin(TAU * 37 * s))
        + 0.15 * math.sin(TAU * 1320 * s)
        for s in t
    ]
    servo = mix(lowpass(servo, 3500), _noise(1.35, seed + 1, 5000), gains=[1.0, 0.08])
    rumble = [math.sin(TAU * 48 * s) * (0.7 + 0.3 * math.sin(TAU * 7 * s)) for s in t]
    rush = _noise(1.35, seed + 2, 1600)
    hiss_hi = _noise(1.35, seed + 3, 7000)
    thruster = mix(rumble, rush, [a - b for a, b in zip(hiss_hi, lowpass(hiss_hi, 2500))], gains=[0.5, 1.4, 0.9])
    n = int(0.6 * RATE)
    hiss_n = lowpass(_noise(0.6, seed + 4, 3200), 3200)
    hiss = [(a - b) * e for a, b, e in zip(hiss_n, lowpass(hiss_n, 1200), _env(n, 0.01, 0.16))]
    hiss = mix(Sfx("tom", 140, dur=0.06).render(), hiss, gains=[0.3, 1.0])
    impact = mix(
        Sfx("kick", 68, dur=0.25).render(),
        _partials([523, 1347, 2209, 3171, 4420], 0.7, 0.22, seed + 5),
        [v * e for v, e in zip(_noise(0.05, seed + 6, 8000), _env(int(0.05 * RATE), 0.001, 0.01))],
        gains=[0.8, 0.7, 0.9],
    )
    tear = mix(
        [v * e for v, e in zip(Sfx("saw", 900, freq_end=260, dur=0.55, cutoff=3000, res=0.5).render(), _env(int(0.6 * RATE), 0.01, 0.3))],
        [v * (1.0 if r.random() < 0.08 else 0.25) for v in _noise(0.7, seed + 7, 6000)],
        _partials([2900, 3700], 0.3, 0.05, seed + 8),
        offsets=[0.0, 0.0, 0.45],
        gains=[0.6, 0.8, 0.4],
    )
    return {
        "stomp": (drive(stomp, 1.8), False),
        "servo": (loopify(servo), True),
        "thruster": (loopify(drive(thruster, 1.5)), True),
        "hiss": (hiss, False),
        "impact": (drive(reverb(impact, room=0.1, wet=0.12), 1.5), False),
        "tear": (drive(tear, 2.0), False),
    }


PACKS = {"mech": mech_pack}


def save_pack(name: str, out_dir: str, prefix: str | None = None, seed: int = 3) -> list[str]:
    """Writes a foley pack's sounds as `<out_dir>/<prefix><sound>.ogg`
    (prefix defaults to `<name>_`)."""
    prefix = f"{name}_" if prefix is None else prefix
    os.makedirs(out_dir, exist_ok=True)
    paths = []
    for sound, (x, loop) in PACKS[name](seed).items():
        p = os.path.join(out_dir, f"{prefix}{sound}.ogg")
        save_ogg(p, x, loop=loop)
        paths.append(p)
    return paths


def _self_test() -> int:
    """Encodes hard cases (peaky explosions with long reverb tails, a full
    square wave, a music loop) and runs check_audio on them."""
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    import check_audio
    big = mix(Sfx("kick", 42, dur=0.6).render(), Sfx("noise_burst", 600, dur=1.0, cutoff=1600, filter_decay=0.4).render(),
              Sfx("tom", 70, dur=0.5).render(), gains=[1.0, 0.9, 0.5])
    song = Song(bpm=128)
    song.track("kick", "x . . . x . . . x . . . x . x .")
    song.track("bass", "A2 . A2 . G2 . F2 . A2 . A2 . G2 . E2 .")
    cases = {
        "boom.ogg": (reverb(mix(Sfx("kick", 58, dur=0.35).render(), Sfx("noise_burst", 900, dur=0.45, cutoff=2200).render()), room=0.6, wet=0.2), {}),
        "boom_tail.ogg": (reverb(big, room=0.9, wet=0.35), {}),
        "die.ogg": (reverb(mix(big, Sfx("saw", 420, freq_end=40, dur=1.3, cutoff=1400).render(), gains=[1.0, 0.45]), room=0.8, wet=0.3), {}),
        "square.ogg": ([0.99 if (i // 100) % 2 else -0.99 for i in range(RATE // 2)], {}),
        "music_loop.ogg": (reverb(song.render(bars=2, loop=True), 0.4, 0.2, loop=True), {"loop": True, "kind": "music"}),
    }
    with tempfile.TemporaryDirectory() as d:
        paths = []
        for name, (x, kw) in cases.items():
            p = os.path.join(d, name)
            save_ogg(p, x, **kw)
            paths.append(p)
        return check_audio.main(paths)


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        sys.exit(_self_test())
    if len(sys.argv) in (4, 5) and sys.argv[1] == "--pack" and sys.argv[2] in PACKS:
        for p in save_pack(sys.argv[2], sys.argv[3], sys.argv[4] if len(sys.argv) == 5 else None):
            print(p)
        sys.exit(0)
    print(__doc__)
