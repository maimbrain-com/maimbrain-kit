"""Renders a game's sound events with its real assets and measures the mix,
since you can't listen to it. assets.md §5 "Mix the sound" has the rules.

    python3 <skill dir>/scripts/mix_check.py games/<name>/assets mix.log
    python3 .../mix_check.py games/<name>/assets mix.log --out mix.wav --json mix.json

The event log is plain text, one event per line (`#` starts a comment),
whitespace- or comma-separated:

    <seconds> <sound> <vol> <pitch> [play|loop|set|stop] [from=<s>] [fade=<s>] [id=<n>]

    0.10 music 0.70 1.0 loop     # a looping voice (music, stems, ambience)
    1.25 pop   0.45 1.12         # a one-shot ("play" is the default)
    3.00 music 0.28 1.0 set      # new vol/pitch for that sound's loop voice
    9.80 music 0 1 stop          # stops the loop (or, for a one-shot, its oldest voice)

Optional fields, for music that moves through a long take (what
`music::Director`'s `Log` writes for a music graph): `from=` starts a
voice that many seconds into its sound (`Sound::play_from`) and `fade=` on
a start is its fade-in (a quarter sine); `fade=` on a stop is a release
(60 dB over that long, `Voice::stop_at`); `id=` names a voice, so `set` and
`stop` with the same id reach that voice even when two of one sound overlap.
A voice started part-way into its sound, or a sound named `*_take`,
`*_take_base` or `*_take_hi` (a music graph's take), is music like a loop.

`sound` is the asset's file name without `.ogg`; vol and pitch are what the
game passes to `Sound::play` (vol 1 = the file's own level, pitch = rate).
Get a log from a bot-played round: the kits' sound modules record one in
tests (`MB_MIX_LOG=/tmp/mix.log cargo test -p <crate> mix_log`), or write
one by hand from the game's cue rates.

It mixes in mono like the platform (master gain 0.8 into a -9 dB 12:1
limiter, 32 voices, oldest one-shot stolen first), measures loudness
K-weighted (LUFS-like: 400 ms momentary, 3 s short-term) per bus and per
sound, and reports: how far the music sits above the summed effects, how
hard the limiter works, each sound's rate, level relative to the music,
share of the effects, overlap, variation, brightness, attack and, for
tonal sounds, whether the notes played sit in the music's key (measured:
generators ignore the key you ask for). It also listens the way a player
does, through a phone speaker: whether each effect's loudest 50 ms is
loud enough, and stands out from the music playing at that moment in
its own frequency band, to be heard at all. Loop voices count as music
unless named with --fx-loop; with several (stems, `mb music --layers`) it
prints how loud each gets against the whole, takes the key from the
track's own layers (not `*_tension` or `*_tick`, which are dread on top of
it) and checks every other layer against that key. Tension (those loops,
and a `*_heartbeat` played over them) is music meant to rise with the
danger: the music's typical level comes from the track alone, and it
reports how much the tension lifts the music on a phone at its loudest
and quietest 3 s (at least +3 dB at its peak, at most +1 dB at its
quietest: dread you can hear, that lets go). Comments starting `# bus`
(what `music::Director`'s `Log` writes for bus changes) are skipped like any
comment. Bus volume and filters (`Bus::Music`) aren't
in the log, so a duck or a muffle doesn't count: a sound must be heard
without them. Exit code 1 if anything is flagged. Decodes with `mb ogg`
(or oggdec, with an older mb); numpy makes it faster but isn't required.
"""

from __future__ import annotations

import argparse
import array
import cmath
import json
import math
import os
import shutil
import subprocess
import sys
import tempfile
import wave

try:
    import numpy as np
except ImportError:  # stdlib fallback
    np = None

MASTER = 0.8  # runtime/src/audio.ts MASTER_GAIN
LIMITER = (-9.0, 6.0, 12.0, 0.003, 0.15)  # threshold dB, knee dB, ratio, attack s, release s
VOICES = 32
BLOCK = 0.1  # loudness blocks; momentary = 4 blocks, short-term = 30
FREQUENT = 40.0  # plays per minute: at or above this a sound is "frequent"
RARE = 6.0  # below this, "rare"
# Rules (assets.md §5), in dB relative to the music's typical short-term level.
RULE_SFX_BUS = 3.0  # music minus all effects (momentary), median, at least
RULE_SFX_P10 = -6.0  # ... and in 90% of the windows
# Fatigue and burying the music come from energy over time, so that's what
# is capped for a frequent sound, not its peak: its own 3 s loudness at -6
# dB adds 1 dB to the music's, about the smallest change anyone notices.
# A short, bright hit fits under this and is still heard (RULE_LOUD); a
# long ring repeated twice a second doesn't.
RULE_FREQUENT = -6.0  # a frequent sound's own short-term level (p90), at most
RULE_REGULAR = -4.0
RULE_REG_HIT = 4.0
RULE_RARE_HIT = 8.0  # a rare sound's loudest moment, at most
# A frequent sound's loudest 400 ms, at most: no louder than a regular one.
# The 400 ms window averages a short hit's attack away (a 50 ms click at
# +8 dB reads about -1 dB here), so this caps a long or doubled-up body,
# not the transient that makes a pickup heard.
RULE_FREQ_HIT = 4.0
RULE_SHARE = 0.5  # share of the effects' energy one frequent sound may take
# Brightness (rms frequency, Hz). A phone speaker and the ear are both most
# sensitive at 1-4 kHz, so that's where a pickup should live; a sound with
# harmonics up there measures 2-4.5 kHz. Past ~5 kHz energy sits in the
# sibilant range, which gets sharp and tiring when it repeats.
RULE_BRIGHT = 5000.0  # Hz for frequent sounds
RULE_HARSH = 6500.0  # Hz for any sound
RULE_OVERLAP = 4  # same-sound voices at once, before it needs a cap
# Heard on a phone. Its speaker barely plays below ~300 Hz (a 4th-order
# roll-off, close to an iPhone's) and is at its best at 1-4 kHz, like the
# ear; the ear's own K-weighting goes on top. Through that, the loudest
# PEAK_WIN of each play (a transient's peak: short enough to catch the
# attack, long enough to resolve the bands) is put against the music in
# third-octave bands (close to the ear's critical bands), twice. Each must
# hold for 3 plays in 4.
PHONE_HP = 300.0  # Hz
PEAK_WIN = 0.05  # s
# - Loud enough: its loudness (`sones`, which counts how a sound spread
#   over many bands is louder than a tone of the same energy) against the
#   music's typical 400 ms loudness, the level the player set the volume
#   by. 10 dB is half as loud. A frequent or regular sound must peak at
#   least half as loud as the music, or it's background; a rare one is news
#   (a fail, a level, a best) and must be close to the music's loudness.
RULE_LOUD = -10.0
RULE_LOUD_RARE = -5.0
# - Not masked: in its best band, among those holding most of it (within
#   10 dB of its strongest), how far it stands over the music in the 400 ms
#   around it. A tone is barely detectable ~4 dB under the noise in its
#   critical band, and recognizing it at a glance takes a few dB over it:
#   +3 dB for frequent and regular sounds, +6 for rare ones.
RULE_HEARD = 3.0
RULE_HEARD_RARE = 6.0
BAND_FLOOR = -30.0  # dB: where the music is silent, a quiet room is about this far under it
# Tension (mb music --layers ...,tension): music that is *supposed* to rise and
# fall with the danger. Loops named so are dread over the track, not the track:
# the key and the music's typical level come from the track's own layers, and
# the tension's rise is reported on its own. A `*_heartbeat` one-shot is part
# of it (scheduled on the grid, more often and louder as the danger grows): it
# counts as music, not as an effect, so it isn't capped like a frequent effect;
# what's checked is that its louder plays are heard on a phone.
TENSION_LAYERS = ("_tension", "_tick")
TENSION_HITS = ("_heartbeat",)
# At its peak (the loudest 3 s), the tension must lift the music by this much
# as heard on a phone: 1 dB is about the smallest change anyone notices, 3 dB
# is a step a player hears without listening for it.
RULE_TENSION_RISE = 3.0
# ... and let go: in its quietest 3 s it may add at most this (dread that never
# stops is just a louder track, and the escape has nothing to release).
RULE_TENSION_FLOOR = 1.0
NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]
MAJOR = [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88]
MINOR = [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17]
SCALE = {"major": {0, 2, 4, 5, 7, 9, 11}, "minor": {0, 2, 3, 5, 7, 8, 10}}


def db(x: float) -> float:
    return 20 * math.log10(max(x, 1e-12))


def lufs(ms: float) -> float:
    return -0.691 + 10 * math.log10(max(ms, 1e-12))


# ---- Assets ----------------------------------------------------------------

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


def decode(path: str) -> tuple[list[float], int]:
    with tempfile.TemporaryDirectory() as d:
        out = os.path.join(d, "x.wav")
        if path.endswith(".wav"):
            out = path
        else:
            _ogg(path, out)
        with wave.open(out) as w:
            n, rate, ch = w.getnframes(), w.getframerate(), w.getnchannels()
            a = array.array("h")
            a.frombytes(w.readframes(n))
    if sys.byteorder == "big":
        a.byteswap()
    if ch == 1:
        return [v / 32768 for v in a], rate
    return [sum(a[i : i + ch]) / (32768 * ch) for i in range(0, len(a), ch)], rate


def biquad(x: list[float], b: tuple, a: tuple) -> list[float]:
    b0, b1, b2 = (v / a[0] for v in b)
    a1, a2 = a[1] / a[0], a[2] / a[0]
    y, x1, x2, y1, y2 = [0.0] * len(x), 0.0, 0.0, 0.0, 0.0
    for i, v in enumerate(x):
        o = b0 * v + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2
        x2, x1, y2, y1 = x1, v, y1, o
        y[i] = o
    return y


def k_filters(rate: int) -> list[tuple]:
    """BS.1770 K-weighting at any rate (the analog prototypes, bilinear): the shelf and the high-pass."""
    w = 2 * math.pi * 1681.974450955533 / rate
    A, al = 10 ** (4.0 / 40), math.sin(w) / (2 * 0.7071752369554196)
    c, s = math.cos(w), 2 * math.sqrt(A) * al
    shelf = ((A * ((A + 1) + (A - 1) * c + s), -2 * A * ((A - 1) + (A + 1) * c), A * ((A + 1) + (A - 1) * c - s)),
             ((A + 1) - (A - 1) * c + s, 2 * ((A - 1) - (A + 1) * c), (A + 1) - (A - 1) * c - s))
    w = 2 * math.pi * 38.13547087602444 / rate
    c, al = math.cos(w), math.sin(w) / (2 * 0.5003270373238773)
    hp = (((1 + c) / 2, -(1 + c), (1 + c) / 2), (1 + al, -2 * c, 1 - al))
    return [shelf, hp]


def k_weight(x: list[float], rate: int) -> list[float]:
    for b, a in k_filters(rate):
        x = biquad(x, b, a)
    return x


def phone_ear(rate: int, n: int) -> tuple[list[float], list[float]]:
    """Power weights for the rfft bins of an n-sample frame: K-weighting (the
    ear), and that times a 4th-order Butterworth high-pass at PHONE_HP (the
    ear listening to a phone speaker)."""
    fs = k_filters(rate)
    ear, phone = [], []
    for k in range(n // 2 + 1):
        f = k * rate / n
        z = cmath.exp(-2j * math.pi * k / n)  # z^-1
        g = 1.0
        for b, a in fs:
            g *= abs((b[0] + b[1] * z + b[2] * z * z) / (a[0] + a[1] * z + a[2] * z * z)) ** 2
        ear.append(g)
        phone.append(g / (1 + (PHONE_HP / max(f, 1e-3)) ** 8))
    return ear, phone


def thirds(rate: int, n: int) -> tuple[list[int], list[float]]:
    """Each rfft bin's third-octave band (-1 outside 100 Hz-8 kHz) and the bands' centres."""
    centres = [1000 * 2 ** (k / 3) for k in range(-10, 10) if 1000 * 2 ** ((k + 0.5) / 3) < rate / 2]
    of = []
    for k in range(n // 2 + 1):
        f = k * rate / n
        of.append(next((i for i, c in enumerate(centres) if c * 2 ** (-1 / 6) <= f < c * 2 ** (1 / 6)), -1))
    return of, centres


def spectra(x, starts: list[int], n: int, wts: list[list[float]], band: list[int], nb: int) -> list[tuple]:
    """For each n-sample Hann frame of x starting at `starts`: for each weighting
    in `wts`, its total mean power, and the first weighting's power per band."""
    win = [0.5 - 0.5 * math.cos(2 * math.pi * k / n) for k in range(n)]
    norm = sum(w * w for w in win) * n / 2
    out = []
    if np is not None and starts:
        idx = np.asarray(starts)[:, None] + np.arange(n)[None, :]
        xa = np.concatenate([np.asarray(x, dtype=np.float64), np.zeros(n)])
        p = np.abs(np.fft.rfft(xa[idx] * np.asarray(win), axis=1)) ** 2 / norm
        tots = [p @ np.asarray(w) for w in wts]
        bw = p * np.asarray(wts[0])
        bi = np.asarray(band)
        bands = np.stack([bw[:, bi == b].sum(axis=1) for b in range(nb)], axis=1)
        return [(tuple(float(t[i]) for t in tots), bands[i].tolist()) for i in range(len(starts))]
    for s in starts:
        frame = [(x[s + k] if s + k < len(x) else 0.0) * win[k] for k in range(n)]
        p = [v / norm for v in power(frame)]
        bands = [0.0] * nb
        for k, v in enumerate(p):
            if band[k] >= 0:
                bands[band[k]] += v * wts[0][k]
        out.append((tuple(sum(v * w for v, w in zip(p, wt)) for wt in wts), bands))
    return out


def bright_hz(x: list[float], rate: int) -> float:
    """RMS frequency: a sine of f Hz has rms(diff)/rms = 2 sin(pi f / rate),
    so inverting that gives a rate-independent brightness (white noise reads
    rate/4)."""
    full = math.sqrt(sum(v * v for v in x) / max(1, len(x)))
    if full < 1e-6:
        return 0.0
    diff = math.sqrt(sum((x[i] - x[i - 1]) ** 2 for i in range(1, len(x))) / max(1, len(x)))
    return rate / math.pi * math.asin(min(1.0, diff / full / 2))


def envelope(x: list[float], rate: int, win: float = 0.005) -> list[float]:
    h = max(1, int(rate * win))
    return [math.sqrt(sum(v * v for v in x[i : i + h]) / h) for i in range(0, len(x), h)]


def power(frame: list[float]) -> list[float]:
    """|FFT|^2 of a power-of-two frame, bins 0..n/2 (iterative radix-2; numpy if present)."""
    if np is not None:
        return list(np.abs(np.fft.rfft(frame)) ** 2)
    n = len(frame)
    a = [complex(v) for v in frame]
    j = 0
    for i in range(1, n):  # bit reversal
        bit = n >> 1
        while j & bit:
            j ^= bit
            bit >>= 1
        j |= bit
        if i < j:
            a[i], a[j] = a[j], a[i]
    size = 2
    while size <= n:
        w = complex(math.cos(2 * math.pi / size), -math.sin(2 * math.pi / size))
        half = size // 2
        tw = [w**k for k in range(half)]
        for start in range(0, n, size):
            for k in range(half):
                u, t = a[start + k], a[start + k + half] * tw[k]
                a[start + k], a[start + k + half] = u + t, u - t
        size *= 2
    return [abs(c) ** 2 for c in a[: n // 2 + 1]]


def chroma(x: list[float], rate: int, secs: float) -> list[float]:
    """Power per pitch class (55 Hz-2 kHz), from Hann frames of 0.37 s over
    the first `secs` (at most 30 s with numpy, 12 s without)."""
    d = max(1, rate // 5512)
    tri = [sum(x[i : i + d]) / d for i in range(len(x) - d + 1)]  # two box passes: less aliasing
    y = [sum(tri[i : i + d]) / d for i in range(0, len(tri) - d + 1, d)]
    r = rate / d
    y = y[: int(min(secs, 30 if np is not None else 12) * r)]
    out = [0.0] * 12
    f = 2048 if len(y) >= 2048 else 1 << max(7, len(y).bit_length() - 1)
    if len(y) < f:
        return out
    win = [0.5 - 0.5 * math.cos(2 * math.pi * k / (f - 1)) for k in range(f)]
    bins = [(k, (round(12 * math.log2(k * r / f / 440)) + 9) % 12) for k in range(1, f // 2) if 55 < k * r / f < 2000]
    for i in range(0, len(y) - f + 1, f // 2):
        p = power([y[i + k] * win[k] for k in range(f)])
        for k, pc in bins:
            out[pc] += p[k]
    return out


def in_scale(ch: list[float], scale, semis: int = 0) -> float:
    """Share of the chroma's power on the scale's notes, played `semis` up
    (`scale` a set of pitch classes, or a list of sets: the best fit)."""
    if isinstance(scale, list):
        return max(in_scale(ch, s, semis) for s in scale)
    tot = sum(ch) or 1e-12
    return sum(ch[i] for i in range(12) if (i + semis) % 12 in scale) / tot


def keys_of(ch: list[float]) -> list[tuple[int, str, float]]:
    """(root, mode, fit) for all 24 keys, best first: the share of power on
    the key's scale (Lyria and friends ignore the key you ask for, so
    measure it), ties between relative keys broken by Krumhansl profiles."""
    def corr(a, b):
        ma, mb = sum(a) / 12, sum(b) / 12
        num = sum((a[i] - ma) * (b[i] - mb) for i in range(12))
        den = math.sqrt(sum((v - ma) ** 2 for v in a) * sum((v - mb) ** 2 for v in b)) or 1e-9
        return num / den

    cand = []
    for r in range(12):
        for m, prof in (("major", MAJOR), ("minor", MINOR)):
            fit = in_scale(ch, {(r + d) % 12 for d in SCALE[m]})
            cand.append((r, m, fit, corr([ch[(i + r) % 12] for i in range(12)], prof)))
    cand.sort(key=lambda t: -(round(t[2], 2) + 0.001 * t[3]))
    return [(r, m, fit) for r, m, fit, _ in cand]


class Asset:
    def __init__(self, path: str):
        self.x, self.rate = decode(path)
        x = self.x
        self.secs = len(x) / self.rate
        self.peak = max((abs(v) for v in x), default=0.0)
        last = max((i for i in range(len(x) - 1, -1, -64) if abs(x[i]) > self.peak * 0.01), default=0)
        self.audible = last / self.rate + 0.01
        self.k = k_weight(x, self.rate)
        self.bright = bright_hz(x, self.rate)
        env = envelope(x[: int(self.rate * 1.0)], self.rate)
        top = max(env, default=0.0)
        i10 = next((i for i, e in enumerate(env) if e >= 0.1 * top), 0)
        i90 = next((i for i, e in enumerate(env) if e >= 0.9 * top), 0)
        self.attack = (i90 - i10) * 5.0
        self.ch = chroma(x, self.rate, 30.0 if self.secs > 3 else min(self.secs, 0.6))
        tot = sum(self.ch) or 1e-9
        # Tonal: one pitch class well above the rest (noise and thuds are flat).
        self.pc = max(range(12), key=lambda i: self.ch[i])
        self.tonal = self.ch[self.pc] / tot >= 0.4 and self.ch[self.pc] >= 3 * sorted(self.ch)[-3]
        if np is not None:
            self.xa, self.ka = np.asarray(self.x, dtype=np.float64), np.asarray(self.k, dtype=np.float64)


# ---- Events and voices -----------------------------------------------------


TAKES = ("_take", "_take_base", "_take_hi")  # a music graph's take (mb music --zones/--sections/--variations)


class Voice:
    def __init__(self, sound: str, t: float, vol: float, pitch: float, loop: bool, frm: float = 0.0, fade: float = 0.0):
        self.sound, self.start, self.loop = sound, t, loop
        self.segs = [(t, vol, pitch)]
        self.end = math.inf
        self.stolen = False
        # Where in the sound it starts (s), its fade-in, and its release (start time, length).
        self.frm, self.fade_in = frm, fade
        self.release: tuple[float, float] | None = None
        self.music = loop or frm > 0 or sound.endswith(TAKES)


def parse(path: str) -> list[tuple[float, str, float, float, str, dict]]:
    ev = []
    with open(path) as f:
        for ln, line in enumerate(f, 1):
            line = line.split("#")[0].replace(",", " ").split()
            if not line:
                continue
            try:
                rest = line[4:]
                op = rest[0] if rest and "=" not in rest[0] else "play"
                extra = {k: float(v) for k, v in (w.split("=", 1) for w in rest if "=" in w)}
                ev.append((float(line[0]), os.path.splitext(os.path.basename(line[1]))[0], float(line[2]), float(line[3]), op, extra))
            except (ValueError, IndexError):
                sys.exit(f"{path}:{ln}: expected '<seconds> <sound> <vol> <pitch> [play|loop|set|stop] [from=<s>] [fade=<s>] [id=<n>]'")
            if op not in ("play", "loop", "set", "stop"):
                sys.exit(f"{path}:{ln}: unknown op {op!r}")
    ev.sort(key=lambda e: e[0])
    return ev


def schedule(ev, assets: dict, end: float) -> tuple[list[Voice], int]:
    voices: list[Voice] = []
    active: list[Voice] = []
    loops: dict[str, Voice] = {}
    by_id: dict[float, Voice] = {}
    steals = 0
    for t, s, vol, pitch, op, extra in ev:
        if op in ("set", "stop"):
            vid = extra.get("id")
            v = by_id.get(vid) if vid is not None else loops.get(s)
            if v is None:
                if op == "stop" and vid is None:  # no loop: stops the oldest one-shot of it still sounding (a voice cap)
                    v = next((v for v in voices if v.sound == s and not v.loop and v.end > t), None)
                    if v is not None:
                        v.end = t
                continue
            if op == "set":
                v.segs.append((t, vol, pitch))
            elif extra.get("fade", 0) > 0:
                # A release: 60 dB over `fade`, then it ends.
                v.release = (t, extra["fade"])
                v.end = min(v.end, t + extra["fade"])
            else:
                v.end = min(v.end, t)
            if op == "stop" and loops.get(s) is v:
                del loops[s]
            continue
        a = assets[s]
        active = [v for v in active if v.end > t]
        if len(active) >= VOICES:
            victim = next((v for v in active if not v.loop), active[0])
            victim.end, victim.stolen = t, True
            active.remove(victim)
            steals += 1
        v = Voice(s, t, vol, pitch, op == "loop", extra.get("from", 0.0), extra.get("fade", 0.0))
        if "id" in extra:
            by_id[extra["id"]] = v
        if v.loop:
            # A loop with an id is a voice of its own (two of one sound may overlap).
            if s in loops and "id" not in extra:
                loops[s].end = t
            loops[s] = v
        else:
            v.end = t + max(0.0, a.secs - v.frm) / max(pitch, 1e-3)
        voices.append(v)
        active.append(v)
    for v in voices:
        v.end = min(v.end, end)
    return voices, steals


def render(v: Voice, a: Asset, rate: int, n_out: int):
    """The voice's raw and K-weighted samples at `rate`, from sample index i0."""
    i0 = int(round(v.start * rate))
    i1 = min(n_out, int(round(v.end * rate)))
    if i1 <= i0:
        return i0, [], []
    bounds = [(max(i0, int(round(t * rate))), vol, p) for t, vol, p in v.segs] + [(i1, 0, 0)]
    pos, n_src = v.frm * a.rate, len(a.x)
    raws, ks = [], []
    for (s0, vol, p), (s1, _, _) in zip(bounds, bounds[1:]):
        m = s1 - s0
        if m <= 0:
            continue
        step = p * a.rate / rate
        if np is not None:
            idx = pos + step * np.arange(m)
            if v.loop:
                idx = np.mod(idx, n_src)
            else:
                ok = idx < n_src - 1
                idx = idx[ok]
            raws.append(vol * np.interp(idx, np.arange(n_src), a.xa))
            ks.append(vol * np.interp(idx, np.arange(n_src), a.ka))
        else:
            r, k = [], []
            x, kx = a.x, a.k
            for j in range(m):
                q = pos + step * j
                if v.loop:
                    q %= n_src
                elif q >= n_src - 1:
                    break
                i = int(q)
                f = q - i
                i2 = (i + 1) % n_src
                r.append(vol * (x[i] + (x[i2] - x[i]) * f))
                k.append(vol * (kx[i] + (kx[i2] - kx[i]) * f))
            raws.append(r)
            ks.append(k)
        pos += step * m
        if not v.loop and pos >= n_src:
            break
    if np is not None:
        raw, kw = (np.concatenate(raws) if raws else np.zeros(0)), (np.concatenate(ks) if ks else np.zeros(0))
    else:
        raw, kw = [y for r in raws for y in r], [y for k in ks for y in k]
    # The fade-in (a quarter sine) and the release (60 dB over its length), as the runtime does them.
    if v.fade_in > 0 or v.release is not None:
        m = len(raw)
        env = [1.0] * m if np is None else np.ones(m)
        if v.fade_in > 0:
            k = min(m, int(v.fade_in * rate))
            if np is not None:
                env[:k] = np.sin(np.pi / 2 * np.arange(k) / (v.fade_in * rate))
            else:
                for j in range(k):
                    env[j] = math.sin(math.pi / 2 * j / (v.fade_in * rate))
        if v.release is not None:
            r0, rl = v.release
            j0 = max(0, int(round(r0 * rate)) - i0)
            tc = rl / 6.9
            if np is not None:
                if j0 < m:
                    env[j0:] *= np.exp(-np.arange(m - j0) / rate / tc)
            else:
                for j in range(j0, m):
                    env[j] *= math.exp(-(j - j0) / rate / tc)
        if np is not None:
            raw, kw = raw * env, kw * env
        else:
            raw, kw = [x * e for x, e in zip(raw, env)], [x * e for x, e in zip(kw, env)]
    return i0, raw, kw


def zeros(n: int):
    return np.zeros(n) if np is not None else array.array("d", bytes(8 * n))


def add(dst, i0: int, src) -> None:
    n = min(len(src), len(dst) - i0)
    if n <= 0:
        return
    if np is not None:
        dst[i0 : i0 + n] += src[:n]
    else:
        for j in range(n):
            dst[i0 + j] += src[j]


def blocks(x, rate: int) -> list[float]:
    h = int(rate * BLOCK)
    if np is not None:
        n = len(x) // h
        return list((np.asarray(x[: n * h]) ** 2).reshape(n, h).mean(axis=1))
    return [sum(v * v for v in x[i : i + h]) / h for i in range(0, len(x) - h + 1, h)]


def phone_blocks(x, rate: int) -> list[float]:
    """Power per block of an (already K-weighted) signal as a phone speaker
    plays it: through PHONE_HP's 4th-order roll-off."""
    h = int(rate * BLOCK)
    if np is not None:
        nb = len(x) // h
        f = np.fft.rfftfreq(h, 1 / rate)
        w = 1 / (1 + (PHONE_HP / np.maximum(f, 1e-3)) ** 8)
        w[1:-1] *= 2  # the bins rfft folds
        p = np.abs(np.fft.rfft(np.asarray(x[: nb * h]).reshape(nb, h), axis=1)) ** 2 @ w / (h * h)
        return list(p)
    w = 2 * math.pi * PHONE_HP / rate
    y = list(x)
    for q in (0.5411961, 1.3065630):  # two Butterworth biquads
        c, al = math.cos(w), math.sin(w) / (2 * q)
        y = biquad(y, ((1 + c) / 2, -(1 + c), (1 + c) / 2), (1 + al, -2 * c, 1 - al))
    return blocks(y, rate)


def windows(ms: list[float], n: int) -> list[float]:
    """Loudness of each n-block window ending at each block."""
    out, acc = [], 0.0
    for i, v in enumerate(ms):
        acc += v
        if i >= n:
            acc -= ms[i - n]
        out.append(lufs(acc / min(i + 1, n)))
    return out


def limiter(x, rate: int) -> tuple[list[float], float]:
    """WebAudio-style compressor on the master: gain reduction (dB) per block, and the peak (dBFS) going in."""
    thr, knee, ratio, att, rel = LIMITER
    ca, cr = math.exp(-1 / (att * rate)), math.exp(-1 / (rel * rate))
    gr, out, h, acc, peak = 0.0, [], int(rate * BLOCK), 0.0, 0.0
    lo = thr - knee / 2
    xs = x.tolist() if np is not None else x
    for i, v in enumerate(xs):
        a = abs(v)
        peak = max(peak, a)
        lvl = 20 * math.log10(a) if a > 1e-6 else -120.0
        if lvl <= lo:
            want = 0.0
        elif lvl < thr + knee / 2:
            want = (1 / ratio - 1) * (lvl - lo) ** 2 / (2 * knee)
        else:
            want = (1 / ratio - 1) * (lvl - thr)
        want = -want
        gr = want + (gr - want) * (ca if want > gr else cr)
        acc += gr
        if (i + 1) % h == 0:
            out.append(acc / h)
            acc = 0.0
    return out, db(peak)


def pct(v: list[float], p: float) -> float:
    s = sorted(v)
    return s[min(len(s) - 1, max(0, int(round((len(s) - 1) * p))))] if s else float("nan")


# Excitation spread between third-octave bands, (bands up, power gain): a
# band masks the ones above it far more than the ones below (Zwicker's
# slopes at moderate levels, about -10 dB a band up and -25 down).
SPREAD = ((1, 0.1), (2, 0.01), (3, 0.001), (-1, 10**-2.5))


def sones(bands: list[float]) -> float:
    """Relative loudness of third-octave band powers (Zwicker, roughly): each
    band's excitation, spread to its neighbours, to the power 0.3, summed.
    So a sound spread over many bands is louder than a pure tone with the
    same energy, as it is to the ear, and 10 dB more of it all is twice as
    loud (Stevens)."""
    nb = len(bands)
    out = 0.0
    for b in range(nb):
        e = bands[b] + sum(bands[b - d] * g for d, g in SPREAD if 0 <= b - d < nb)
        out += max(e, 0.0) ** 0.3
    return out


class Phone:
    """The music as the player hears it on a phone: band levels over the
    400 ms around each frame, its typical loudness, and a floor where it's
    silent. `hear` puts an effect's plays against it (nothing to put them
    against without music: the effects are then the mix)."""

    def __init__(self, mus, i_from: int, n: int, rate: int):
        self.rate = rate
        self.fn = 1 << round(math.log2(rate * PEAK_WIN))
        ear, phone = phone_ear(rate, self.fn)
        self.wts = [phone, ear]
        self.band, self.centres = thirds(rate, self.fn)
        nb = self.nb = len(self.centres)
        self.hop = self.fn // 2 if np is not None else self.fn * 2  # pure Python: fewer, sparser frames
        self.i_from = i_from
        sp = spectra(mus, list(range(i_from, max(i_from, n - self.fn), self.hop)), self.fn, self.wts, self.band, nb)
        h = max(1, int(round(0.2 * rate / self.hop)))
        tot, self.bands = [], []
        for i in range(len(sp)):
            w = sp[max(0, i - h) : i + h + 1]
            tot.append(sum(s[0][0] for s in w) / len(w))
            self.bands.append([sum(s[1][b] for s in w) / len(w) for b in range(nb)])
        top = max(tot, default=0.0)
        on = [i for i, t in enumerate(tot) if t > top * 1e-6] if top > 0 else []
        typ = [sum(self.bands[i][b] for i in on) / len(on) for b in range(nb)] if on else [0.0] * nb
        self.floor = [max(v, 1e-12) * 10 ** (BAND_FLOOR / 10) for v in typ]
        self.loud = pct([sones(self.bands[i]) for i in on], 0.5) if on else 0.0

    def hear(self, own, vs: list[Voice], a: Asset, q: float = 0.25) -> dict | None:
        """Each play's loudest 50 ms (up to 64 plays): its loudness against the
        music's typical loudness and its best band's margin over the music
        playing then, both in dB and met by 3 plays in 4 (the `q` quantile:
        0.75 for a heartbeat, whose quiet plays are meant to be quiet); how
        much of it the phone speaker loses (dB) and its strongest band (Hz)."""
        plays = [v for v in vs if not v.loop]
        plays = plays[:: max(1, len(plays) // 64)] if self.loud > 0 else []
        fn, rate = self.fn, self.rate
        e2 = np.concatenate([[0.0], np.cumsum(np.asarray(own) ** 2)]) if np is not None else None
        starts = []
        for v in plays:
            i0 = int(round(v.start * rate))
            i1 = min(len(own) - fn, i0 + int(rate * min(1.0, a.audible / max(v.segs[0][2], 1e-3))))
            if i1 < i0:
                continue
            cand = range(i0, i1 + 1, fn // 4)
            if e2 is not None:
                starts.append(max(cand, key=lambda j: e2[j + fn] - e2[j]))
            else:
                starts.append(max(cand, key=lambda j: sum(own[k] * own[k] for k in range(j, j + fn))))
        loud, margin, loss, hz = [], [], [], []
        for j, ((p, e), sb) in zip(starts, spectra(own, starts, fn, self.wts, self.band, self.nb)):
            top = max(sb)
            if p <= 0 or top <= 0:
                continue
            mb = self.bands[min(len(self.bands) - 1, max(0, round((j - self.i_from) / self.hop)))] if self.bands else [0.0] * self.nb
            loud.append(10 * math.log2(sones(sb) / self.loud))
            margin.append(max(10 * math.log10(sb[b] / max(mb[b], self.floor[b])) for b in range(self.nb) if sb[b] >= 0.1 * top))
            loss.append(10 * math.log10(p / max(e, 1e-30)))
            hz.append(self.centres[sb.index(top)])
        if not loud:
            return None
        return {"loud_db": round(pct(loud, q), 1), "heard_db": round(pct(margin, q), 1),
                "phone_loss_db": round(pct(loss, 0.5), 1), "band_hz": round(pct(hz, 0.5))}


# ---- Report ----------------------------------------------------------------


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("assets", help="the game's assets/ directory")
    ap.add_argument("log", help="event log (see the module docstring)")
    ap.add_argument("--out", help="write the mixed round (after the limiter) as a WAV")
    ap.add_argument("--json", help="write the measurements as JSON")
    ap.add_argument("--rate", type=int, default=22050, help="render rate (default 22050)")
    ap.add_argument("--tail", type=float, default=1.5, help="seconds rendered after the last event")
    ap.add_argument("--fx-loop", default="", help="comma-separated loops that are effects, not music (a ticking clock, an engine, rain)")
    args = ap.parse_args(argv)
    ev = parse(args.log)
    if not ev:
        sys.exit("no events")
    names = sorted({e[1] for e in ev})
    assets = {}
    for s in names:
        for ext in (".ogg", ".wav"):
            p = os.path.join(args.assets, s + ext)
            if os.path.exists(p):
                assets[s] = Asset(p)
                break
        else:
            sys.exit(f"no asset for sound {s!r} in {args.assets}")
    t0, t1 = ev[0][0], ev[-1][0] + args.tail
    rate = args.rate
    n = int(math.ceil(t1 * rate))
    voices, steals = schedule(ev, assets, t1)
    fx_loops = {x for x in args.fx_loop.split(",") if x}
    music = {v.sound for v in voices if v.music and v.sound not in fx_loops}
    # Heartbeats over a tension layer are part of the tension music (with no tension layer, an effect).
    hits = {s for s in names if s.endswith(TENSION_HITS) and s not in music} if any(s.endswith(TENSION_LAYERS) for s in music) else set()
    mix, mus_k, sfx_k, mus_raw = zeros(n), zeros(n), zeros(n), zeros(n)
    per: dict[str, object] = {}
    ph: dict[str, list[float]] = {}  # each music sound's power per block as a phone plays it
    heard: dict[str, dict | None] = {}
    phone = None
    # The music first, then the tension's hits: effects (and hits) are heard against the music.
    for s in sorted(names, key=lambda s: (s not in music, s not in hits)):
        own, own_raw = zeros(n), zeros(n)
        vs = [v for v in voices if v.sound == s]
        for v in vs:
            i0, raw, kw = render(v, assets[s], rate, n)
            add(mix, i0, raw)
            add(own_raw, i0, raw)
            add(own, i0, kw)
        add(mus_k if s in music or s in hits else sfx_k, 0, own)
        per[s] = blocks(own, rate)
        if s in music or s in hits:
            ph[s] = phone_blocks(own, rate)
        if s in music:
            add(mus_raw, 0, own_raw)
        elif s not in fx_loops:
            phone = phone or (Phone(mus_raw, int(t0 * rate), n, rate) if music else None)
            # A heartbeat is meant to be quiet while the danger is low: judge its louder plays.
            heard[s] = phone.hear(own_raw, vs, assets[s], 0.75 if s in hits else 0.25) if phone else None
    if np is not None:  # master gain
        mix *= MASTER
    else:
        for i in range(n):
            mix[i] *= MASTER
    gr, peak_in = limiter(mix, rate)
    mb, sb = blocks(mus_k, rate), blocks(sfx_k, rate)
    b0 = int(t0 / BLOCK)
    mm, sm = windows(mb, 4)[b0:], windows(sb, 4)[b0:]
    mst = windows(mb, 30)[b0:]
    has_music = any(v > -70 for v in mm)
    span_min = max(1e-3, (ev[-1][0] - t0) / 60)
    flags: list[str] = []
    rep: dict = {"seconds": round(t1 - t0, 1), "voice_steals": steals, "peak_into_limiter_db": round(peak_in, 1)}

    gr_play = gr[b0:]
    rep["limiter"] = {"over_1db": round(sum(g > 1 for g in gr_play) / max(1, len(gr_play)), 3),
                      "over_3db": round(sum(g > 3 for g in gr_play) / max(1, len(gr_play)), 3), "max_db": round(max(gr_play, default=0), 1)}
    if rep["limiter"]["over_3db"] > 0.05:
        flags.append(f"master limiter pulls >3 dB {100 * rep['limiter']['over_3db']:.0f}% of the time: everything (music too) pumps; turn effects down")
    if steals:
        flags.append(f"{steals} voice(s) stolen at the 32-voice limit: cap simultaneous voices per sound")

    # The track's own layers; the tension (layers and hits) rises over them.
    core = {s for s in music if not s.endswith(TENSION_LAYERS)} or music
    tens = sorted((music - core) | hits)
    if has_music:
        act = [i for i, v in enumerate(mm) if v > -70]
        margin = [mm[i] - sm[i] for i in act]
        # The music's typical level is the track's, not the tension's: dread rising over it
        # mustn't move the level effects are judged against, nor make the calm look ducked.
        cb = [sum(c) for c in zip(*(per[s] for s in core))]
        cst, cm = windows(cb, 30)[b0:], windows(cb, 4)[b0:]
        ref = pct([cst[i] for i in act], 0.5)
        rep["music"] = {"sounds": sorted(music), "short_term_lufs_median": round(ref, 1), "p10": round(pct([cst[i] for i in act], 0.1), 1),
                        "over_sfx_db_median": round(pct(margin, 0.5), 1), "over_sfx_db_p10": round(pct(margin, 0.1), 1),
                        "buried_pct": round(100 * sum(m < 0 for m in margin) / len(margin), 1),
                        "ducked_pct": round(100 * sum(cm[i] < ref - 4 for i in act) / len(act), 1)}
        if tens:
            # How much the tension lifts the music, over 3 s, as a phone plays it (and to the ear).
            def rise(get):
                c = [sum(v) for v in zip(*(get(s) for s in core))]
                t = [sum(v) for v in zip(*(get(s) for s in tens))]
                out, ac, at_ = [], 0.0, 0.0
                for i in range(len(c)):
                    ac, at_ = ac + c[i], at_ + t[i]
                    if i >= 30:
                        ac, at_ = ac - c[i - 30], at_ - t[i - 30]
                    out.append(10 * math.log10((ac + at_) / ac) if ac > 1e-12 else 0.0)
                return [out[i + b0] for i in act if i + b0 < len(out)]
            rp, rk = rise(lambda s: ph[s]), rise(lambda s: per[s])
            top = max(range(len(rp)), key=lambda i: rp[i]) if rp else 0
            rep["tension"] = {"sounds": tens, "phone_rise_peak_db": round(max(rp, default=0), 1), "phone_rise_low_db": round(min(rp, default=0), 1),
                              "rise_peak_db": round(max(rk, default=0), 1), "peak_at_seconds": round(max(t0 + 1.5, (act[top] + b0 + 1) * BLOCK - 1.5), 1) if rp else None}
            tr = rep["tension"]
            if tr["phone_rise_peak_db"] < RULE_TENSION_RISE:
                flags.append(f"tension: at its peak it lifts the music only {tr['phone_rise_peak_db']:+.1f} dB on a phone (want >= {RULE_TENSION_RISE:+.0f}): "
                             "take the tension layer to vol 1.4 at full danger, bring the clock in, play the heartbeat louder")
            if tr["phone_rise_low_db"] > RULE_TENSION_FLOOR:
                flags.append(f"tension never lets go: even at its quietest it adds {tr['phone_rise_low_db']:+.1f} dB (want <= {RULE_TENSION_FLOOR:+.0f}): "
                             "bring it to 0 when the danger is gone, so the escape is heard")
        if rep["music"]["over_sfx_db_median"] < RULE_SFX_BUS:
            flags.append(f"music is under the summed effects most of the time (median {rep['music']['over_sfx_db_median']:+.1f} dB; want >= {RULE_SFX_BUS:+.0f})")
        if rep["music"]["over_sfx_db_p10"] < RULE_SFX_P10:
            flags.append(f"music is buried by effects in 10% of the round (p10 {rep['music']['over_sfx_db_p10']:+.1f} dB; want >= {RULE_SFX_P10:+.0f})")
    else:
        flags.append("no music (no loop events): levels below are relative to -23 LUFS, and no effect is checked for being heard over it")
        ref = -23.0
    # Effects are judged against the music's typical level (ducking under a
    # big moment is allowed and shouldn't make that moment look louder).

    sfx_total = sum(sb) or 1e-12
    mus_now = windows(mb, 4)
    if has_music:
        # Tension layers (mb music --layers …,tension: <name>_tension, <name>_tick) are dread
        # on top of the track: the key and the main loop come from the track's own layers,
        # and the tension layers are checked against them like any other layer.
        main = max(core, key=lambda s: sum(per[s]))
        # How loud each loop gets, against the whole music's typical level.
        lv = {}
        for s in sorted(music):
            st = windows(per[s], 30)
            on = [st[i] for i in range(b0, len(st)) if st[i] > -70]
            if on:
                lv[s] = round(pct(on, 0.9) - ref, 1)
        rep["music"]["layers_p90_db"] = lv
        # The key of everything musical, weighted by how loud each loop plays.
        mch = [0.0] * 12
        for s in core:
            v = next(v for v in voices if v.sound == s)
            d, ch = round(12 * math.log2(max(v.segs[0][2], 1e-3))), assets[s].ch
            w = sum(per[s]) / (sum(ch) or 1e-12)
            for i in range(12):
                mch[(i + d) % 12] += w * ch[i]
        ks = keys_of(mch)
        (kr, km, kc), (kr2, km2, kc2) = ks[:2]
        # A chromatic track can fit two scales about equally: then a note is
        # in key if either has it.
        scale, key_names = [], []
        for r, mo, f in ks:
            sc = {(r + d) % 12 for d in SCALE[mo]}
            if f >= kc - 0.02 and sc not in scale and len(scale) < 2:
                scale.append(sc)
                key_names.append(f"{NAMES[r]} {mo}")
        guess = key_names[0] + (f" or {key_names[1]}" if len(key_names) > 1 else "")
        rep["key"] = {"guess": guess, "confidence": round(kc, 2), "runner_up": f"{NAMES[kr2]} {km2} ({kc2:.2f})", "from": "+".join(sorted(core))}
        base = in_scale(assets[main].ch, scale)
        for s in sorted(music - {main}):
            v = next(v for v in voices if v.sound == s)
            semis = round(12 * math.log2(max(v.segs[0][2], 1e-3)))
            if keys_of(assets[s].ch)[0][2] < 0.75:
                continue  # unpitched (a ticking clock, wind): no key to clash
            share = in_scale(assets[s].ch, scale, semis)
            rep.setdefault("layers", {})[s] = {"in_key": round(share, 2), "main_in_key": round(base, 2), "semitones": semis}
            best = max(range(-6, 6), key=lambda d: in_scale(assets[s].ch, scale, d))
            if share < 0.7 or share < in_scale(assets[s].ch, scale, best) - 0.08:
                flags.append(f"{s}: music layer clashes with {main} ({100 * share:.0f}% of it in key at {semis:+d} semitones vs {100 * base:.0f}%; {best:+d} fits best)")
    rows = []
    for s in names:
        if s in music:
            continue
        vs = [v for v in voices if v.sound == s]
        a = assets[s]
        st = windows(per[s], 30)
        mo = windows(per[s], 4)
        on = [i for i in range(b0, len(st)) if st[i] > -70]
        rel_st = pct([st[i] - ref for i in on], 0.9) if on else float("nan")
        rel_hit = max((mo[i] - ref for i in on), default=float("nan"))
        gains = [db(v.segs[0][1]) for v in vs]
        pitches = [v.segs[0][2] for v in vs]
        g_mean = sum(gains) / len(gains)
        g_sd = math.sqrt(sum((g - g_mean) ** 2 for g in gains) / len(gains))
        lp = [math.log2(p) for p in pitches]
        p_sd = 100 * (2 ** math.sqrt(sum((x - sum(lp) / len(lp)) ** 2 for x in lp) / len(lp)) - 1)
        spans = sorted((v.start, min(v.end, v.start + a.audible / max(v.segs[0][2], 1e-3))) for v in vs)
        overlap, ends = 0, []
        for st0, en in spans:
            ends = [e for e in ends if e > st0] + [en]
            overlap = max(overlap, len(ends))
        rate_min = len(vs) / span_min
        cls = "tension" if s in hits else "loop" if s in fx_loops else "frequent" if rate_min >= FREQUENT else "rare" if rate_min < RARE else "regular"
        med_p = sorted(pitches)[len(pitches) // 2]
        row = {"sound": s, "class": cls, "plays": len(vs), "per_min": round(rate_min, 1), "share_pct": 0.0 if s in hits else round(100 * sum(per[s]) / sfx_total, 1),
               "st_vs_music_db": round(rel_st, 1), "hit_vs_music_db": round(rel_hit, 1), "vol_db_spread": round(g_sd, 2), "pitch_spread_pct": round(p_sd, 1),
               "pitch_range": [round(min(pitches), 3), round(max(pitches), 3)], "overlap": overlap, "bright_hz": round(a.bright * med_p),
               "attack_ms": round(a.attack), "stolen": sum(v.stolen for v in vs)}
        notes = []
        h = heard.get(s)
        if h:
            row.update(h)
            want_loud, want_heard = (RULE_LOUD_RARE, RULE_HEARD_RARE) if cls == "rare" else (RULE_LOUD, RULE_HEARD)
            quiet = []
            if h["loud_db"] < want_loud:
                quiet.append(f"too quiet to hear on a phone (peaks {h['loud_db']:+.1f} dB against the music's loudness; want >= {want_loud:+.0f})")
            if h["heard_db"] < want_heard:
                quiet.append(f"masked by the music on a phone (in its best band it stands {h['heard_db']:+.1f} dB over it; want >= {want_heard:+.0f})")
            if quiet:
                if h["phone_loss_db"] < -6:
                    fix = f"the speaker loses {-h['phone_loss_db']:.0f} dB of it (it's mostly under ~{PHONE_HP:.0f} Hz); pitch it up an octave or two or use a brighter take"
                elif h["heard_db"] < want_heard:
                    fix = f"it sits ~{h['band_hz']} Hz, where the music is busy; move it where the music is thinner (1-4 kHz suits a phone), add a click on the attack, or raise it"
                else:
                    fix = "raise it, or give it harmonics or a click at 1-4 kHz (a bright sound is louder than a pure tone of the same energy)"
                notes.append(" and ".join(quiet) + ": " + fix)
        if cls == "frequent":
            if rel_st > RULE_FREQUENT:
                notes.append(f"too loud for how often it plays ({rel_st:+.1f} dB vs music; want <= {RULE_FREQUENT:+.0f})")
            if rel_hit > RULE_FREQ_HIT:
                notes.append(f"its loudest moment is {rel_hit:+.1f} dB over the music (want <= {RULE_FREQ_HIT:+.0f})")
            if row["share_pct"] > 100 * RULE_SHARE and len(names) - len(music) > 2:
                notes.append(f"dominates the effects ({row['share_pct']:.0f}% of their energy)")
            if row["bright_hz"] > RULE_BRIGHT:  # a fast attack is what a pickup wants, unless it's this bright
                notes.append(f"bright for a frequent sound ({row['bright_hz']} Hz{', hard attack' if a.attack < 2 else ''}; want <= {RULE_BRIGHT:.0f}: lowpass above ~6 kHz or a softer take)")
            if g_sd < 0.4 and p_sd < 1.0:
                notes.append("no variation: randomize pitch ±3-5% and vol ±1-2 dB")
            if overlap > RULE_OVERLAP:
                notes.append(f"{overlap} voices at once: cap it (steal the oldest)")
        elif cls in ("regular", "loop"):
            if rel_st > RULE_REGULAR:
                notes.append(f"loud for a sound heard every few seconds ({rel_st:+.1f} dB vs music; want <= {RULE_REGULAR:+.0f})")
            if rel_hit > RULE_REG_HIT:
                notes.append(f"its loudest moment is {rel_hit:+.1f} dB over the music (want <= {RULE_REG_HIT:+.0f})")
            if overlap > RULE_OVERLAP + 2 and cls == "regular":
                notes.append(f"{overlap} voices at once: cap it")
        elif rel_hit > RULE_RARE_HIT:
            notes.append(f"very loud ({rel_hit:+.1f} dB over the music): turn it down or duck the music under it")
        if a.bright * med_p > RULE_HARSH:
            notes.append(f"harsh ({row['bright_hz']} Hz)")
        if max(pitches) > 1.6 or min(pitches) < 0.6:
            notes.append(f"pitched {min(pitches):.2f}-{max(pitches):.2f}x: samples sound chipmunked/sludgy past ~±7 semitones")
        # Key only matters while the music is heard.
        with_music = [v.segs[0][2] for v in vs if mus_now[min(len(mus_now) - 1, int(v.start / BLOCK))] > ref - 20]
        if has_music and a.tonal and with_music:
            pitches = with_music
            offs = sorted({round(12 * math.log2(p)) for p in pitches})
            row["notes_played"] = [NAMES[(a.pc + d) % 12] for d in offs]
            bad = [(d, in_scale(a.ch, scale, d)) for d in offs if in_scale(a.ch, scale, d) < 0.7]
            if bad:
                notes.append(f"tonal (mostly {NAMES[a.pc]}) and clashes with {rep['key']['guess']} at " + ", ".join(f"{d:+d} st ({100 * k:.0f}% in key)" for d, k in bad))
            detuned = sum(abs(12 * math.log2(p) - round(12 * math.log2(p))) > 0.3 for p in pitches)
            if detuned > len(pitches) * 0.25:
                notes.append(f"tonal and {detuned} of {len(pitches)} plays fall between semitones: jitter tonal sounds ≤1% or in scale steps")
        row["notes"] = notes
        flags += [f"{s}: {x}" for x in notes]
        rows.append(row)
    rep["sounds"] = rows
    rep["flags"] = flags

    print(f"{rep['seconds']} s, {len(voices)} voices, peak into limiter {peak_in:+.1f} dBFS, limiter >1 dB {100 * rep['limiter']['over_1db']:.0f}% / >3 dB {100 * rep['limiter']['over_3db']:.0f}% (max {rep['limiter']['max_db']} dB)")
    if has_music:
        m = rep["music"]
        k = rep["key"]
        print(f"music {'+'.join(m['sounds'])}: {m['short_term_lufs_median']} LUFS short-term (p10 {m['p10']}), ducked {m['ducked_pct']}% of the time")
        if len(m["layers_p90_db"]) > 1:
            print("music layers at their loudest (3 s, p90) vs the music: " + ", ".join(f"{s} {v:+.1f} dB" for s, v in m["layers_p90_db"].items()))
        print(f"key of {k['from']} ~{k['guess']} ({k['confidence']}; next {k['runner_up']})" + "".join(f"; {s} {100 * l['in_key']:.0f}% in key at {l['semitones']:+d} st" for s, l in rep.get("layers", {}).items()))
        print(f"music over all effects: median {m['over_sfx_db_median']:+.1f} dB, p10 {m['over_sfx_db_p10']:+.1f} dB, effects louder {m['buried_pct']}% of the time")
        if "tension" in rep:
            tr = rep["tension"]
            print(f"tension ({'+'.join(tr['sounds'])}) lifts the music on a phone by {tr['phone_rise_peak_db']:+.1f} dB at its peak (3 s around {tr['peak_at_seconds']} s; "
                  f"{tr['rise_peak_db']:+.1f} dB to the ear) and {tr['phone_rise_low_db']:+.1f} dB at its quietest")
    print(f"\n{'sound':12} {'class':8} {'/min':>6} {'share':>6} {'st/mus':>7} {'hit/mus':>7} {'loud':>6} {'heard':>6} {'phone':>6} {'±vol':>5} {'±pitch':>6} {'ovl':>3} {'bright':>6} {'atk':>4}")
    for r in sorted(rows, key=lambda r: -r["share_pct"]):
        ph = f"{r['loud_db']:+6.1f} {r['heard_db']:+6.1f} {r['phone_loss_db']:+6.1f}" if "loud_db" in r else f"{'':6} {'':6} {'':6}"
        print(f"{r['sound'][:12]:12} {r['class']:8} {r['per_min']:6.1f} {r['share_pct']:5.1f}% {r['st_vs_music_db']:+7.1f} {r['hit_vs_music_db']:+7.1f} {ph} "
              f"{r['vol_db_spread']:5.1f} {r['pitch_spread_pct']:5.1f}% {r['overlap']:3d} {r['bright_hz']:6d} {r['attack_ms']:4d}"
              + (f"  notes {' '.join(r['notes_played'])}" if r.get("notes_played") else ""))
    print("\nst/mus: the sound's own 3 s loudness (p90 while it plays) minus the music's; hit/mus: its loudest 400 ms minus the music's;")
    print("on a phone speaker, 3 plays in 4: loud: its loudest 50 ms against the music's typical loudness (-10 is half as loud);")
    print("heard: how far that stands over the music playing then, in its best third-octave band; phone: dB of it the speaker loses;")
    print("±vol dB and ±pitch % are the spread across plays; ovl: most voices of it at once; bright: rms frequency (Hz); atk: attack ms.")
    print("\n" + ("\n".join("FLAG " + f for f in flags) if flags else "mix looks sane"))
    if args.json:
        with open(args.json, "w") as f:
            json.dump(rep, f, indent=1)
    if args.out:
        h = int(rate * BLOCK)
        with wave.open(args.out, "wb") as w:
            w.setnchannels(1)
            w.setsampwidth(2)
            w.setframerate(rate)
            out = array.array("h")
            for i in range(n):
                g = 10 ** (-gr[min(i // h, len(gr) - 1)] / 20) if gr else 1.0
                out.append(int(max(-1.0, min(1.0, float(mix[i]) * g)) * 32767))
            w.writeframes(out.tobytes())
    return 1 if flags else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
