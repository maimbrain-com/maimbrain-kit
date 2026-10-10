#!/usr/bin/env python3
"""Swerve's OLD code-made sounds and music -> assets/*.ogg (public domain).

The kit now ships generated audio (tools/regen.sh). This is the free, offline
fallback: it overwrites the same files, but makes no wind, pass or blinker
(those then stay silent) and its music is in A, so set MUSIC_ROOT in
src/sound.rs back to 0 (coin.ogg here is an A5 pluck).

    python3 tools/gen_audio.py      # encodes with `mb ogg` (or oggenc)

Everything is in A major, so the coin chime (stepped up the A major
pentatonic by the game, one step per coin in a row) sits inside the music.

  engine      a 1 s engine loop (the game pitches it with the speed)
  start       the engine revving up + a go chime
  lane        a short tyre swish (pitch varied per lane change)
  squeal      a tyre squeal (quick reversals, oil)
  coin        a bright pluck on A5
  near        near miss: a whoosh and a ding
  boost       a rising zap
  smash       boosting through something: a crunch and a clank
  speedup     a rising arpeggio
  alert       a two-tone warning (new hazard, blinker)
  oil         a slippery wobble
  crash       the big crunch with glass
  over        a sad descending wah
  best        new best fanfare
  ui          a click
  music_base  "Sunset Drive", 120 bpm, 4 bars: bass, kick, pad, lead
  music_hi    the tension layer for the same 4 bars: hats, claps, arpeggio
"""

import math
import os
import random
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
for _p in (HERE, os.path.join(HERE, "..", "..", "..", "scripts")):  # scaffolded games get copies in tools/
    if os.path.exists(os.path.join(_p, "sfx.py")):
        sys.path.insert(0, _p)
        break
from sfx import RATE, Inst, Sfx, Song, lowpass, mix, reverb, save_ogg, save_stems  # noqa: E402

OUT = os.path.join(HERE, "..", "assets")
# Short files at a modest quality: the kit ships inside the mb binary.
Q = 2


def noise(dur, seed, decay=1.0):
    rnd = random.Random(seed)
    n = int(dur * RATE)
    return [rnd.uniform(-1, 1) * (1 - i / n) ** decay for i in range(n)]


def env(x, attack=0.01):
    a = max(1, int(attack * RATE))
    return [v * min(1.0, i / a) for i, v in enumerate(x)]


def save(name, samples, **kw):
    save_ogg(os.path.join(OUT, name + ".ogg"), samples, quality=Q, **kw)


def engine():
    """A seamless 1 s loop: whole numbers of cycles of every partial."""
    n = RATE
    out = []
    for i in range(2 * n):  # two periods: the filter settles in the first, we keep the second
        t = i / RATE
        putt = 0.65 + 0.35 * math.sin(math.tau * 24 * t)  # cylinders firing
        v = 0.0
        for k, (f, g) in enumerate(((60, 1.0), (120, 0.6), (180, 0.35), (240, 0.22), (300, 0.12))):
            ph = (f * t) % 1.0
            v += g * (2 * ph - 1) * (0.7 if k else 1.0)
        out.append(v * putt)
    out = lowpass(out, 900)[n:]
    save("engine", out, loop=True)


def main():
    os.makedirs(OUT, exist_ok=True)
    engine()

    rev = Sfx(Inst("saw", wave2="square", detune=12, adsr=(0.02, 0.2, 0.8, 0.15), cutoff=1400), 70, freq_end=210, dur=0.55).render()
    go = Sfx("bell", 880, dur=0.35).render()
    save("start", reverb(mix(rev, go, offsets=[0.0, 0.45], gains=[0.8, 0.5]), room=0.3, wet=0.15))

    swish = env(lowpass(noise(0.16, 1, 1.6), 2600), 0.02)
    save("lane", mix(swish, Sfx("sine", 420, freq_end=300, dur=0.08, adsr=(0.002, 0.07, 0.0, 0.02)).render(), gains=[1.0, 0.25]))

    squeal = Sfx(Inst("square", wave2="sine", duty=0.3, detune=30, adsr=(0.01, 0.1, 0.7, 0.1), cutoff=3500, vibrato=(0.6, 18.0, 0.0), noise=0.2), 1650, freq_end=1450, dur=0.32).render()
    save("squeal", squeal)

    save("coin", mix(Sfx("pluck", 880, dur=0.16).render(), Sfx("bell", 1760, dur=0.14).render(), gains=[1.0, 0.35]))

    whoosh = env(lowpass(noise(0.35, 2, 0.8), 1800), 0.12)
    ding = Sfx("bell", 1318.5, dur=0.3).render()
    save("near", reverb(mix(whoosh, ding, offsets=[0.0, 0.12], gains=[0.9, 0.55]), room=0.35, wet=0.18))

    zap = Sfx(Inst("saw", wave2="square", detune=20, adsr=(0.01, 0.3, 0.6, 0.2), cutoff=4000, res=0.3), 220, freq_end=1320, dur=0.5).render()
    save("boost", reverb(mix(zap, env(lowpass(noise(0.6, 3, 1.0), 3000), 0.2), gains=[0.7, 0.5]), room=0.4, wet=0.15))

    thump = Sfx("kick", 60, dur=0.25).render()
    clank = Sfx("bell", 523.25, dur=0.25, detune=1200 * 1.41).render()
    save("smash", mix(lowpass(noise(0.3, 4, 2.0), 3000), thump, clank, gains=[0.9, 0.9, 0.35]))

    save("speedup", Sfx("pluck", 440, dur=0.07, arp=(0, 4, 7, 12), arp_step=0.06).render())

    save("alert", Sfx(Inst("square", duty=0.4, adsr=(0.004, 0.05, 0.7, 0.04), cutoff=2600), 659.25, dur=0.24, arp=(0, 5), arp_step=0.12).render())

    save("oil", Sfx(Inst("sine", wave2="triangle", detune=15, adsr=(0.01, 0.2, 0.6, 0.15), vibrato=(1.2, 9.0, 0.0)), 520, freq_end=230, dur=0.5).render())

    boom = Sfx("kick", 45, dur=0.6).render()
    crunch = lowpass(noise(0.7, 5, 1.6), 2400)
    glass = [v * 0.6 for v in noise(0.6, 6, 2.5)]
    tink = mix(*(Sfx("bell", f, dur=0.2).render() for f in (2637, 3136, 2349)), offsets=[0.08, 0.17, 0.26])
    save("crash", reverb(mix(boom, crunch, glass, tink, offsets=[0, 0, 0.04, 0.0], gains=[1.0, 0.9, 0.4, 0.3]), room=0.5, wet=0.2))

    wah = Inst("saw", wave2="square", detune=8, adsr=(0.01, 0.2, 0.7, 0.15), cutoff=1200, vibrato=(0.25, 6.0, 0.0))
    over = mix(*(Sfx(wah, f, freq_end=f * 0.95, dur=0.3).render() for f in (659.25, 587.33, 554.37, 440.0)), offsets=[0.0, 0.32, 0.64, 0.96])
    save("over", reverb(over, room=0.5, wet=0.2))

    fan = Inst("square", wave2="square", duty=0.25, detune=8, adsr=(0.004, 0.1, 0.6, 0.12), cutoff=3500)
    best = mix(*(Sfx(fan, f, dur=d).render() for f, d in ((440.0, 0.12), (554.37, 0.12), (659.25, 0.12), (880.0, 0.45))), offsets=[0.0, 0.12, 0.24, 0.36])
    save("best", reverb(best, room=0.4, wet=0.2))

    save("ui", Sfx(Inst("square", duty=0.2, adsr=(0.001, 0.025, 0.0, 0.01), cutoff=3000), 1200, dur=0.03).render())

    # "Sunset Drive": A – F#m – D – E, a bar each, 120 bpm (8 s).
    song = Song(bpm=120)
    song.track("bass", "A2 . A3 . A2 . A3 . A2 . A3 . A2 . A3 . F#2 . F#3 . F#2 . F#3 . F#2 . F#3 . F#2 . F#3 . "
                       "D2 . D3 . D2 . D3 . D2 . D3 . D2 . D3 . E2 . E3 . E2 . E3 . E2 . E3 . E2 . E3 .", vol=0.45, name="base")
    song.track("kick", "x . . . x . . . x . . . x . . .", vol=0.5, name="base")
    song.track("pad", "C#4 - - - - - - - - - - - - - - - C#4 - - - - - - - - - - - - - - - "
                      "D4 - - - - - - - - - - - - - - - B3 - - - - - - - - - - - - - - -", vol=0.13, name="base")
    song.track("pad", "E4 - - - - - - - - - - - - - - - A4 - - - - - - - - - - - - - - - "
                      "F#4 - - - - - - - - - - - - - - - E4 - - - - - - - - - - - - - - -", vol=0.11, name="base")
    song.track("lead", "E5 - . C#5 . E5 . F#5 - - . E5 . C#5 . . C#5 - . A4 . C#5 . E5 - - . F#5 . E5 . . "
                       "D5 - . F#5 . A5 . F#5 - - . E5 . D5 . . E5 - . B4 . E5 . G#5 - - - - . . . .", vol=0.2, name="base")
    song.track("hat", ". x . x . x . x . x . x . x . X", vol=0.22, name="hi")
    song.track("clap", ". . . . x . . . . . . . x . . x?", vol=0.3, name="hi")
    song.track("arp", "A4 C#5 E5 A5 A4 C#5 E5 A5 A4 C#5 E5 A5 A4 C#5 E5 A5 F#4 A4 C#5 F#5 F#4 A4 C#5 F#5 F#4 A4 C#5 F#5 F#4 A4 C#5 F#5 "
                      "D4 F#4 A4 D5 D4 F#4 A4 D5 D4 F#4 A4 D5 D4 F#4 A4 D5 E4 G#4 B4 E5 E4 G#4 B4 E5 E4 G#4 B4 E5 E4 G#4 B4 E5", vol=0.13, name="hi")
    stems = song.stems(bars=4, groups={"base": ["base"], "hi": ["hi"]})
    stems["base"] = reverb(stems["base"], 0.3, 0.12, loop=True)
    # The stems are the biggest files: quality 0 keeps them ~45 KB each.
    save_stems(stems, {"base": os.path.join(OUT, "music_base.ogg"), "hi": os.path.join(OUT, "music_hi.ogg")}, quality=0)
    print("wrote", ", ".join(sorted(f for f in os.listdir(OUT) if f.endswith(".ogg"))))


if __name__ == "__main__":
    main()
