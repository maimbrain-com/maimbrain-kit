#!/usr/bin/env python3
"""Nova Pip's sound effects, synthesized: an analog-synth palette made to
sit inside the synthwave music (detuned saws and pulses, FM bells,
filtered-noise sweeps, sub drops, gated-reverb snaps, dotted-eighth echoes
at the music's 120 BPM). Deterministic: the same script always writes the
same sounds, and every tonal sound is tuned to the music's key.

    python3 tools/synth_sfx.py            every effect -> assets/*.ogg
    python3 tools/synth_sfx.py pop gem    just these

THE KEY. The main loop measures as G minor (chroma: G, Bb, D, Eb strongest;
the bass walks F - G - G - Eb per bar), whatever key its prompt asked for,
so the effects are in G minor: the pop ends on G4, the gem rings on G5,
the stingers use G minor's notes, and src/sound.rs climbs the pop and gem
ladders up the G minor pentatonic (G Bb C D F). The boss loop centres on A
(A and E strongest, Bb and G# around it), so while it plays sound.rs moves
the tonal effects up a whole tone and the boss-only sounds (charge, rage,
bhit) are on A. If you regenerate the music, measure its key again (a
chroma of the decoded loop) and change ROOT / BOSS_ROOT here and KEY in
src/sound.rs.

Levels: each file is loudness-normalized to about -14 dBFS RMS (the skill's
sfx.py); the mix (which sound sits how far under the music) is in
src/sound.rs. Check them with the skill's check_audio.py and the round mix
with tools/mixsim.py.

Needs python3 with numpy; encodes with `mb ogg` (or oggenc/oggdec).
"""

import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
for _p in (HERE, os.path.join(HERE, "..", "..", "..", "scripts")):  # scaffolded games get a copy in tools/
    if os.path.exists(os.path.join(_p, "sfx.py")):
        sys.path.insert(0, _p)
        break
import sfx  # noqa: E402

SR = 32000
sfx.RATE = SR  # write 32 kHz files (small; nothing here needs more)
OUT = os.environ.get("SFX_OUT") or os.path.join(HERE, "..", "assets")
QUALITY = 3

A4 = 440.0
NOTES = {"C": -9, "D": -7, "E": -5, "F": -4, "G": -2, "A": 0, "B": 2}


def hz(name):
    """'G4' = 392.0, 'Bb3', 'F#5'."""
    s = NOTES[name[0]]
    rest = name[1:]
    while rest and rest[0] in "#b":
        s += 1 if rest[0] == "#" else -1
        rest = rest[1:]
    return A4 * 2 ** ((s + 12 * (int(rest) - 4)) / 12)


ROOT = "G"  # the main loop's key: G minor
BOSS_ROOT = "A"  # the boss loop's centre

# --- oscillators -------------------------------------------------------------


def n_(d):
    return int(round(d * SR))


def t_(d):
    return np.arange(n_(d)) / SR


def _f(freq, n):
    return np.broadcast_to(np.asarray(freq, dtype=np.float64), (n,)) if np.ndim(freq) == 0 else np.asarray(freq, dtype=np.float64)


def _blep(ph, dt):
    """PolyBLEP correction for a phase in [0, 1) with increment dt."""
    y = np.zeros_like(ph)
    a = ph < dt
    x = ph[a] / dt[a]
    y[a] = x + x - x * x - 1
    b = ph > 1 - dt
    x = (ph[b] - 1) / dt[b]
    y[b] = x * x + x + x + 1
    return y


def saw(freq, n, ph0=0.0):
    f = _f(freq, n)
    dt = f / SR
    ph = (ph0 + np.cumsum(dt) - dt) % 1.0
    return 2 * ph - 1 - _blep(ph, dt)


def pulse(freq, n, duty=0.5, ph0=0.0):
    f = _f(freq, n)
    dt = f / SR
    ph = (ph0 + np.cumsum(dt) - dt) % 1.0
    d = _f(duty, n)
    y = np.where(ph < d, 1.0, -1.0) + _blep(ph, dt) - _blep((ph - d) % 1.0, dt)
    return y - (2 * d - 1)


def sine(freq, n, ph0=0.0):
    f = _f(freq, n)
    return np.sin(2 * np.pi * (ph0 + np.cumsum(f) / SR))


def tri(freq, n):
    f = _f(freq, n)
    ph = (np.cumsum(f) / SR) % 1.0
    return 4 * np.abs(ph - 0.5) - 1


def fm(freq, n, ratio, index):
    """A two-operator FM voice; `index` may be an envelope."""
    f = _f(freq, n)
    pm = 2 * np.pi * np.cumsum(f * ratio) / SR
    return np.sin(2 * np.pi * np.cumsum(f) / SR + _f(index, n) * np.sin(pm))


def supersaw(freq, n, voices=5, spread=14.0, seed=1):
    """Detuned saws (cents spread) with random start phases: the synthwave pad/brass."""
    rng = np.random.default_rng(seed)
    f = _f(freq, n)
    out = np.zeros(n)
    for k in range(voices):
        c = spread * (k - (voices - 1) / 2) / max(1, (voices - 1) / 2)
        out += saw(f * 2 ** (c / 1200), n, rng.random())
    return out / np.sqrt(voices)


def noise(n, seed=1):
    return np.random.default_rng(seed).uniform(-1, 1, n)


def glide(f0, f1, n, curve=1.0):
    """Exponential pitch glide f0 -> f1 over n samples (curve > 1: faster at first)."""
    x = np.linspace(0, 1, n) ** (1 / curve)
    return f0 * (f1 / f0) ** x


# --- filters and envelopes -----------------------------------------------------


def svf(x, fc, q=0.707, mode="lp"):
    """A TPT state-variable filter (stable at any cutoff); fc may sweep."""
    n = len(x)
    fc = np.clip(_f(fc, n), 20, SR * 0.45)
    g = np.tan(np.pi * fc / SR)
    k = 1 / q
    a1 = 1 / (1 + g * (g + k))
    a2 = g * a1
    a3 = g * a2
    out = np.empty(n)
    ic1 = ic2 = 0.0
    lp, bp = mode == "lp", mode == "bp"
    for i in range(n):
        v3 = x[i] - ic2
        v1 = a1[i] * ic1 + a2[i] * v3
        v2 = ic2 + a2[i] * ic1 + a3[i] * v3
        ic1 = 2 * v1 - ic1
        ic2 = 2 * v2 - ic2
        out[i] = v2 if lp else v1 if bp else x[i] - k * v1 - v2
    return out


def lp1(x, fc):
    a = 1 - np.exp(-2 * np.pi * fc / SR)
    y = np.empty_like(x)
    s = 0.0
    for i, v in enumerate(x):
        s += a * (v - s)
        y[i] = s
    return y


def env(n, a=0.002, tau=0.1, hold=0.0, end=None):
    """Linear attack, optional hold, exponential decay (tau s), a short fade at the end."""
    t = np.arange(n) / SR
    e = np.where(t < a, t / max(a, 1e-6), np.exp(-np.maximum(0, t - a - hold) / tau))
    e[(t >= a) & (t < a + hold)] = 1.0
    f = min(n, n_(0.004))
    e[n - f :] *= np.linspace(1, 0, f)
    return e


def adsr(n, a, d, s, r, gate):
    t = np.arange(n) / SR
    e = np.interp(t, [0, a, a + d, gate, gate + r], [0, 1, s, s, 0], right=0)
    return e


def pad(x, d):
    return np.concatenate([x, np.zeros(n_(d))])


def at(dst, src, t, g=1.0):
    i = n_(t)
    m = min(len(src), len(dst) - i)
    if m > 0:
        dst[i : i + m] += g * src[:m]
    return dst


def layer(d, *parts):
    """Mixes (offset s, gain, signal) parts into a buffer d seconds long."""
    out = np.zeros(n_(d))
    for o, g, s in parts:
        at(out, s, o, g)
    return out


# --- space ---------------------------------------------------------------------


def _conv(x, ir):
    n = len(x) + len(ir) - 1
    m = 1 << (n - 1).bit_length()
    return np.fft.irfft(np.fft.rfft(x, m) * np.fft.rfft(ir, m), m)[:n]


def reverb(x, decay=1.2, wet=0.2, damp=5000.0, seed=11):
    """A dense plate-like reverb: x convolved with exponentially decaying
    filtered noise (decay = RT60 seconds)."""
    t = t_(decay * 1.1)
    ir = noise(len(t), seed) * np.exp(-6.9 * t / decay)
    ir = lp1(ir, damp)
    ir /= np.sqrt(np.sum(ir**2))
    y = _conv(x, ir)
    return pad(x, len(ir) / SR)[: len(y)] + wet * y[: len(y)] * 4


def gated(x, length=0.16, wet=0.5, damp=7000.0, seed=12):
    """The 1980s gated reverb: a dense, non-decaying burst of reflections
    that is cut off hard after `length` seconds (the snare 'snap')."""
    t = t_(length + 0.012)
    shape = np.where(t < length, 0.8 + 0.2 * t / length, np.maximum(0, 1 - (t - length) / 0.012))
    ir = lp1(noise(len(t), seed) * shape, damp)
    ir /= np.sqrt(np.sum(ir**2))
    y = _conv(x, ir)
    return pad(x, len(ir) / SR)[: len(y)] + wet * y * 3


def echo(x, time=0.375, fb=0.35, wet=0.35, repeats=4, damp=3500.0):
    """A tape-ish delay, darker on each repeat; 0.375 s = a dotted eighth at 120 BPM."""
    d = n_(time)
    out = pad(x, time * repeats + 0.05)
    tap = x.copy()
    for k in range(1, repeats + 1):
        tap = lp1(tap, damp) * fb
        at(out, tap * wet / fb, k * time)
    return out


def drive(x, k=2.0):
    return np.tanh(x * k) / np.tanh(k)


def trim(x, floor_db=-60.0):
    """Drops the silent end."""
    lim = np.max(np.abs(x)) * 10 ** (floor_db / 20)
    idx = np.where(np.abs(x) > lim)[0]
    return x[: idx[-1] + 1 + n_(0.01)] if len(idx) else x


# --- the sounds ----------------------------------------------------------------

R = hz(ROOT + "4")  # G4


def shot():
    """Auto-fire: plays ~4-5 times a second all round, so it is tiny and
    dark: a pulse 'pew' gliding G6 -> G5 in 40 ms through a 2.4 kHz low-pass,
    a soft 3 ms attack and no noise."""
    n = n_(0.075)
    f = glide(hz("G6"), hz("G5"), n, curve=3)
    v = pulse(f, n, duty=0.3) * 0.6 + sine(f / 2, n) * 0.5
    v = svf(v, glide(3200, 1400, n), q=0.8)
    return v * env(n, a=0.003, tau=0.022)


def hit():
    """An enemy takes a hit and lives: a soft dull tick (band-passed noise
    and a short falling sine), so a stream of them reads as patter."""
    n = n_(0.06)
    tick = svf(noise(n, 3), 1500, q=1.6, mode="bp") * env(n, a=0.001, tau=0.008)
    body = sine(glide(620, 300, n), n) * env(n, a=0.001, tau=0.014)
    return tick * 0.8 + body * 0.7


def bhit():
    """A shot lands on the queen: a heavier, darker armoured thunk on A
    (the boss loop's key) with a hint of metal."""
    n = n_(0.13)
    body = sine(glide(hz("A3"), hz("A2"), n, 2), n) * env(n, a=0.001, tau=0.03)
    metal = fm(hz("A4"), n, 3.51, 1.4 * env(n, a=0.001, tau=0.02)) * env(n, a=0.001, tau=0.025)
    thud = svf(noise(n, 4), 900, q=1.2, mode="bp") * env(n, a=0.001, tau=0.012)
    return body * 0.8 + metal * 0.25 + thud * 0.6


def pop():
    """An enemy is destroyed (it replaces the old crunch): a bright
    detuned-saw zap diving two octaves onto G4, a short noise burst through
    a falling band-pass, and a gated-reverb snap. The game pitches it by
    enemy type and climbs the G minor pentatonic with the combo."""
    n = n_(0.2)
    f = glide(hz("G6"), R, n_(0.09), curve=2.5)
    f = np.concatenate([f, np.full(n - len(f), R)])
    z = (saw(f, n) + saw(f * 2 ** (9 / 1200), n, 0.3) + 0.5 * pulse(f / 2, n, 0.5)) / 2.5
    z = svf(z, glide(7000, 1300, n, 2), q=1.4) * env(n, a=0.0015, tau=0.055)
    m = n_(0.12)
    b = svf(noise(m, 5), glide(3200, 600, m, 2), q=1.3, mode="bp") * env(m, a=0.001, tau=0.025)
    x = layer(0.32, (0, 1.0, gated(z, length=0.1, wet=0.3)), (0, 0.55, b))
    return trim(x)


def boom():
    """A big enemy or a chain explosion: noise through a low-pass falling
    5 kHz -> 150 Hz, a sub drop G2 -> G1, and a gated snap on top."""
    d = 0.9
    n = n_(d)
    roar = svf(noise(n, 6), glide(5000, 150, n, 2.2), q=0.9) * env(n, a=0.002, tau=0.22)
    sub = sine(glide(hz("G2"), hz("G1"), n_(0.45), 1.5), n_(0.45)) * env(n_(0.45), a=0.002, tau=0.16)
    snap = gated(svf(noise(n_(0.03), 7), 2500, q=0.8, mode="bp") * env(n_(0.03), a=0.001, tau=0.01), length=0.14, wet=0.8)
    x = layer(d, (0, 1.0, roar), (0, 1.1, sub), (0, 0.6, snap))
    return trim(drive(x, 1.6))


def boss_boom():
    """The queen explodes: a long noise sweep, a deep sub drop, a G minor
    supersaw chord falling an octave as it dies, a gated snap and a big
    reverb tail. The main loop comes back 1.6 s later, also in G minor."""
    d = 2.6
    n = n_(d)
    roar = svf(noise(n, 8), glide(8000, 80, n, 2.5), q=0.9) * env(n, a=0.003, tau=0.6)
    m = n_(1.4)
    sub = sine(glide(hz("G2"), 38.0, m, 1.3), m) * env(m, a=0.003, tau=0.5)
    chord = np.zeros(n)
    fall = glide(1.0, 0.5, n, 1.6)
    for k, nm in enumerate(("G2", "D3", "G3", "Bb3", "D4")):
        chord += supersaw(hz(nm) * fall, n, voices=3, spread=12, seed=20 + k)
    chord = svf(chord, glide(3500, 220, n, 1.5), q=1.1) * env(n, a=0.01, tau=0.7)
    snap = gated(svf(noise(n_(0.04), 9), 2000, q=0.8, mode="bp") * env(n_(0.04), a=0.001, tau=0.015), length=0.2, wet=0.9)
    x = layer(d, (0, 1.0, roar), (0, 1.2, sub), (0.02, 0.35, chord), (0, 0.7, snap))
    return trim(reverb(drive(x, 1.4), decay=1.6, wet=0.22))


def die():
    """Pip is shot down (the slow-motion spin): an impact, then the ship's
    synth powering down, a detuned saw stack gliding G4 -> G1 like a tape
    stopping. Little booms and the big one land on top from the game."""
    d = 2.0
    n = n_(d)
    f = glide(hz("G4"), hz("G1"), n, 0.7)
    wob = 1 + 0.012 * np.sin(2 * np.pi * np.cumsum(glide(5, 14, n)) / SR)
    v = supersaw(f * wob, n, voices=3, spread=18, seed=30) + 0.6 * pulse(f / 2, n, 0.4)
    v = svf(v, glide(4500, 250, n, 0.8), q=1.6) * adsr(n, 0.01, 0.2, 0.75, 0.35, 1.6)
    hitn = n_(0.25)
    imp = svf(noise(hitn, 31), glide(3000, 300, hitn), q=1.0) * env(hitn, a=0.001, tau=0.05)
    thump = sine(glide(150, 45, hitn), hitn) * env(hitn, a=0.001, tau=0.07)
    x = layer(d, (0, 0.55, v), (0, 1.0, gated(imp, 0.15, 0.6)), (0, 1.0, thump))
    return trim(reverb(x, decay=1.0, wet=0.15))


def hurt():
    """Pip loses a heart: a gated noise smack and a square-wave 'bwow'
    falling D5 -> D3 with a fast wobble, over a low thump. Loud and rare."""
    d = 0.55
    n = n_(d)
    m = n_(0.3)
    f = glide(hz("D5"), hz("D3"), m, 1.4) * 2 ** (np.sin(2 * np.pi * 28 * t_(0.3)) * 0.8 / 12)
    z = svf(pulse(f, m, 0.35), glide(5000, 900, m), q=2.0) * env(m, a=0.002, tau=0.11)
    k = n_(0.12)
    smack = svf(noise(k, 40), glide(4000, 600, k), q=1.0, mode="bp") * env(k, a=0.001, tau=0.03)
    thump = sine(glide(140, 50, n_(0.2)), n_(0.2)) * env(n_(0.2), a=0.001, tau=0.06)
    x = layer(d, (0, 0.8, z), (0, 1.0, gated(smack, 0.13, 0.7)), (0, 0.9, thump))
    return trim(drive(x, 1.5))


def gem():
    """A gem: a soft FM bell on G5 (the index decays fast, so it starts
    glassy and rings pure), low-passed; the game climbs it up the G minor
    pentatonic with the streak."""
    n = n_(0.32)
    f = hz("G5")
    b = fm(f, n, 2.0, 2.2 * env(n, a=0.001, tau=0.03)) * env(n, a=0.003, tau=0.08)
    b += 0.25 * sine(2 * f, n) * env(n, a=0.002, tau=0.04)
    return svf(b, 4500, q=0.7)


def power():
    """A power-up: a G minor arpeggio (G4 Bb4 D5 G5 Bb5 D6) in thirty-second
    notes at 120 BPM on a PWM pulse lead, with dotted-eighth echoes. The
    game plays a heart a fourth higher (C minor, still in key)."""
    step = 0.0625
    notes = ("G4", "Bb4", "D5", "G5", "Bb5", "D6")
    d = step * len(notes) + 0.35
    out = np.zeros(n_(d))
    for k, nm in enumerate(notes):
        L = step * (1 if k < len(notes) - 1 else 5)
        n = n_(L + 0.06)
        v = pulse(hz(nm), n, duty=0.25 + 0.1 * np.sin(2 * np.pi * 3 * t_(L + 0.06))) + 0.5 * saw(hz(nm) * 1.004, n)
        v = svf(v, 900 + 4000 * env(n, a=0.001, tau=0.04), q=1.2) * adsr(n, 0.002, 0.05, 0.6, 0.05, L)
        at(out, v, k * step, 0.6 + 0.08 * k)
    return trim(echo(out, 0.375, fb=0.32, wet=0.3, repeats=3))


def shield():
    """The shield takes the hit: a cluster of FM bells on D6, G6 and Bb6
    sparkling down, a falling noise shimmer, a low G thump, gated."""
    d = 0.8
    out = np.zeros(n_(d))
    rng = np.random.default_rng(50)
    for k, nm in enumerate(("D6", "G6", "Bb6", "G5", "D6", "Bb5", "G5")):
        n = n_(0.35)
        b = fm(hz(nm), n, 3.0, 1.5 * env(n, a=0.001, tau=0.02)) * env(n, a=0.001, tau=0.08)
        at(out, b, k * 0.035 + rng.uniform(0, 0.01), 0.45 - 0.04 * k)
    m = n_(0.4)
    sh = svf(noise(m, 51), glide(7000, 1500, m), q=1.5, mode="bp") * env(m, a=0.002, tau=0.1)
    th = sine(glide(hz("G2") * 2, hz("G2"), n_(0.25)), n_(0.25)) * env(n_(0.25), a=0.001, tau=0.08)
    x = layer(d, (0, 1.0, svf(out, 6000)), (0, 0.5, sh), (0, 0.9, th))
    return trim(gated(x, 0.14, 0.3))


def shield_hum():
    """The shield's loop: a low G2 / D3 / G3 detuned-saw drone through a
    low-pass that pulses in eighth notes (4 Hz at 120 BPM). Exactly 2 s
    with every frequency a multiple of 0.5 Hz, so it loops seamlessly."""
    L = 2.0
    n3 = n_(3 * L)  # render three loops, keep the middle one (filter settled)
    t = np.arange(n3) / SR
    v = np.zeros(n3)
    for f0, g in ((98.0, 1.0), (147.0, 0.6), (196.0, 0.35)):
        for det in (-0.5, 0.5):
            v += g * saw(np.full(n3, f0 + det), n3)
    lfo = 0.5 - 0.5 * np.cos(2 * np.pi * 4.0 * t)  # 4 Hz: eighth notes
    v = svf(v, 380 + 520 * lfo, q=1.3)
    v *= 0.8 + 0.2 * lfo
    v += 0.5 * sine(np.full(n3, 98.0), n3)
    return v[n_(L) : n_(2 * L)]


def graze():
    """A bullet passes close: a quick resonant band-passed whoosh sweeping
    up and down past you, and a faint D5 glint (the fifth of G minor)."""
    d = 0.3
    n = n_(d)
    t = t_(d)
    sweep = 600 * (2600 / 600) ** np.sin(np.pi * np.minimum(1, t / 0.22))
    w = svf(noise(n, 60), sweep, q=2.2, mode="bp") * adsr(n, 0.04, 0.05, 0.6, 0.15, 0.1)
    m = n_(0.2)
    p = sine(hz("D5"), m) * env(m, a=0.004, tau=0.05)
    return layer(d, (0, 1.0, w), (0.03, 0.35, p))


def lock():
    """A darter locks on: two quick square beeps, D5 then G5 (a rising
    fourth), with a dotted-eighth echo."""
    out = np.zeros(n_(0.2))
    for k, nm in enumerate(("D5", "G5")):
        n = n_(0.055)
        v = svf(pulse(hz(nm), n, 0.5), 2800, q=0.8) * adsr(n, 0.002, 0.02, 0.7, 0.01, 0.045)
        at(out, v, k * 0.075)
    return trim(echo(out, 0.375, fb=0.25, wet=0.25, repeats=2))


def charge():
    """The queen charges an attack (0.8 s telegraph): an A2 / A3 detuned
    saw swell, its low-pass rising 200 Hz -> 3 kHz and its pitch bending up
    a fifth at the end, cut off as she fires."""
    d = 0.8
    n = n_(d)
    bend = 2 ** (7 / 12 * np.clip((t_(d) - 0.45) / 0.35, 0, 1) ** 2)
    v = supersaw(hz("A2") * bend, n, voices=3, spread=16, seed=70) + 0.7 * supersaw(hz("A3") * bend, n, voices=3, spread=10, seed=71)
    v = svf(v, glide(200, 3000, n, 0.6), q=3.0)
    return v * adsr(n, 0.5, 0.1, 1.0, 0.03, d - 0.03)


def rage():
    """The queen enrages (below 40 %): an A minor supersaw stab bending
    down a whole tone, a sub drop and a gated snap, with a long echo."""
    d = 1.0
    n = n_(d)
    bend = glide(1.0, 2 ** (-2 / 12), n, 1.2)
    v = sum(supersaw(hz(nm) * bend, n, voices=5, spread=16, seed=80 + k) for k, nm in enumerate(("A2", "E3", "A3", "C4", "E4")))
    v = svf(v, glide(5000, 400, n, 1.4), q=1.4) * env(n, a=0.004, tau=0.25)
    m = n_(0.5)
    sub = sine(glide(hz("A2"), hz("A1"), m), m) * env(m, a=0.002, tau=0.15)
    snap = gated(svf(noise(n_(0.03), 81), 1800, q=0.8, mode="bp") * env(n_(0.03), a=0.001, tau=0.01), 0.16, 0.9)
    x = layer(d, (0, 0.45, v), (0, 1.0, sub), (0, 0.7, snap))
    return trim(echo(drive(x, 1.3), 0.341, fb=0.3, wet=0.25, repeats=3))  # 132 BPM dotted eighth


def new():
    """A new enemy type: a two-note brass stab, Eb5 falling to D5 (the
    minor sixth resolving to the fifth: ominous), supersaw with gated snap."""
    out = np.zeros(n_(0.6))
    for k, (nm, L) in enumerate((("Eb5", 0.14), ("D5", 0.3))):
        n = n_(L + 0.1)
        v = supersaw(hz(nm), n, voices=5, spread=18, seed=90 + k) + 0.6 * supersaw(hz(nm) / 2, n, voices=3, spread=10, seed=95 + k)
        v = svf(v, 800 + 4200 * env(n, a=0.002, tau=0.06), q=1.2) * adsr(n, 0.004, 0.08, 0.7, 0.08, L)
        at(out, v, k * 0.16)
    return trim(echo(gated(out, 0.12, 0.35), 0.375, fb=0.3, wet=0.3, repeats=2))


def warn():
    """The WARNING before the queen: a detuned-saw siren with two rising
    sweeps, G3 -> D4 and then A3 -> E4, landing on the boss loop's A."""
    d = 2.3
    n = n_(d)
    t = t_(d)
    f = np.where(t < 1.1, hz("G3") * (hz("D4") / hz("G3")) ** np.clip(t / 0.9, 0, 1), hz("A3") * (hz("E4") / hz("A3")) ** np.clip((t - 1.1) / 0.9, 0, 1))
    v = supersaw(f, n, voices=5, spread=20, seed=100) + 0.6 * pulse(f / 2, n, 0.5)
    trem = 0.75 + 0.25 * np.cos(2 * np.pi * 8 * t)  # sixteenths at 120 BPM
    v = svf(v, 700 + 2600 * (0.5 - 0.5 * np.cos(2 * np.pi * t / 1.1)), q=2.5) * trem
    e = adsr(n, 0.05, 0.2, 0.9, 0.25, d - 0.25) * np.where((t > 1.0) & (t < 1.1), 0.3, 1.0)
    return trim(reverb(v * e, decay=1.0, wet=0.2))


def start():
    """A round starts: a noise riser into a G power-chord stab (G4 D5 G5)
    with a gated snap and a dotted-eighth echo."""
    d = 1.1
    rn = n_(0.35)
    riser = svf(noise(rn, 110), glide(500, 6000, rn, 0.7), q=2.0, mode="bp") * adsr(rn, 0.3, 0.02, 1.0, 0.03, 0.32)
    sn = n_(0.4)
    stab = sum(supersaw(hz(nm), sn, voices=5, spread=16, seed=111 + k) for k, nm in enumerate(("G3", "G4", "D5", "G5")))
    stab = svf(stab, 900 + 5000 * env(sn, a=0.001, tau=0.08), q=1.1) * adsr(sn, 0.003, 0.1, 0.5, 0.15, 0.2)
    x = layer(d, (0, 0.5, riser), (0.33, 0.5, gated(stab, 0.12, 0.4)))
    return trim(echo(x, 0.375, fb=0.3, wet=0.25, repeats=2))


def over():
    """Game over: three falling notes, D5 Bb4 G4, on a soft pulse lead with
    a closing filter and dotted-eighth echoes; the last one held, with
    vibrato."""
    out = np.zeros(n_(1.0))
    for k, (nm, L) in enumerate((("D5", 0.2), ("Bb4", 0.2), ("G4", 0.55))):
        n = n_(L + 0.15)
        t = t_(L + 0.15)
        f = hz(nm) * 2 ** (0.25 * np.sin(2 * np.pi * 5.5 * t) * (t > 0.15) / 12)
        v = pulse(f, n, 0.3) + 0.6 * saw(f * 1.003, n)
        v = svf(v, 600 + 2500 * env(n, a=0.002, tau=0.12), q=1.0) * adsr(n, 0.006, 0.1, 0.7, 0.15, L)
        at(out, v, k * 0.25)
    return trim(echo(out, 0.375, fb=0.3, wet=0.3, repeats=3))


def best():
    """A new best: the synthwave cadence Eb - F - G (bVI - bVII - I) as
    rising supersaw arpeggios landing on a bright G major chord, gated and
    echoed."""
    s = 0.0625
    seq = [("Eb4", "G4", "Bb4", "Eb5"), ("F4", "A4", "C5", "F5")]
    d = 2.2
    out = np.zeros(n_(d))
    for c, notes in enumerate(seq):
        for k, nm in enumerate(notes):
            n = n_(s + 0.06)
            v = supersaw(hz(nm), n, voices=3, spread=12, seed=120 + 4 * c + k) + 0.6 * pulse(hz(nm), n, 0.25)
            v = svf(v, 1200 + 4000 * env(n, a=0.001, tau=0.03), q=1.1) * adsr(n, 0.002, 0.04, 0.6, 0.04, s)
            at(out, v, (c * 4 + k) * s, 0.55 + 0.04 * k)
    n = n_(1.4)
    ch = sum(supersaw(hz(nm), n, voices=5, spread=16, seed=130 + k) for k, nm in enumerate(("G3", "B3", "D4", "G4", "B4", "D5")))
    ch = svf(ch, 1000 + 4500 * env(n, a=0.002, tau=0.25), q=1.0) * adsr(n, 0.004, 0.3, 0.55, 0.5, 0.8)
    at(out, gated(ch, 0.14, 0.35), 8 * s, 0.5)
    return trim(echo(out, 0.375, fb=0.3, wet=0.22, repeats=2))


def ui():
    """A button: a soft muted triangle tap on G4 with a little felt click."""
    n = n_(0.09)
    v = tri(R, n) * env(n, a=0.002, tau=0.025)
    c = lp1(noise(n, 140), 1500) * env(n, a=0.001, tau=0.006)
    return svf(v + 0.6 * c, 2500)


SOUNDS = {
    "shot": shot, "hit": hit, "bhit": bhit, "pop": pop, "boom": boom, "boss_boom": boss_boom, "die": die,
    "hurt": hurt, "gem": gem, "power": power, "shield": shield, "shield_hum": shield_hum, "graze": graze,
    "lock": lock, "charge": charge, "rage": rage, "new": new, "warn": warn, "start": start, "over": over,
    "best": best, "ui": ui,
}
LOOPS = {"shield_hum"}


def main(names):
    for nm in names or SOUNDS:
        if nm not in SOUNDS:
            raise SystemExit(f"unknown sound {nm!r}; one of {', '.join(SOUNDS)}")
        x = SOUNDS[nm]()
        path = os.path.join(OUT, nm + ".ogg")
        sfx.save_ogg(path, list(x), loop=nm in LOOPS, quality=QUALITY)
        print(f"{path}: {len(x) / SR:.2f} s, {os.path.getsize(path) // 1024} KB")
        gen = path + ".gen.json"
        if os.path.exists(gen):  # the old ElevenLabs sidecar no longer describes this file
            os.remove(gen)


if __name__ == "__main__":
    main(sys.argv[1:])
