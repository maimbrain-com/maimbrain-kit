#!/usr/bin/env python3
"""Bun Run's code-made effects -> assets/*.ogg (public domain, offline, free).

The frequent, tonal effects are synthesized here rather than generated, so
their pitch, length and brightness are exact: they're in the music's key and
soft enough to fire a few times a second under it. Everything else comes
from `mb sfx` in tools/regen.sh.

    python3 tools/gen_sfx.py       # encodes with `mb ogg` (or oggenc)

The music (music_base/music_hi) is in E, and borrows from both E major and
E minor (its lead has G and G#, C and C#; the skill's scripts/mix_check.py
reads it as E minor), so everything here uses only the notes the two share,
E F# A B:

  coin       a soft, round two-note blip, B5 -> E6, triangle with a hint of
             pulse, 0.12 s. The game climbs it gently per coin in a row
             (LADDER in src/sound.rs: 0, 5, 7 semitones = E-A-B, grace notes
             B-E-F#).
  arc        the "collect streak": a quick soft sparkle (B5 E6 F#6 B6) when
             every coin of an arc is taken, 0.4 s.
  milestone  a short fanfare on the chip lead (E5 B5 E6, every 100 m), 0.6 s.
  new        a two-note "heads up" (B5 B5 E6), a new obstacle family, 0.35 s.
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
for _p in (HERE, os.path.join(HERE, "..", "..", "..", "scripts")):  # scaffolded games get copies in tools/
    if os.path.exists(os.path.join(_p, "sfx.py")):
        sys.path.insert(0, _p)
        break
from sfx import Inst, echo, lowpass, mix, note_freq, save_ogg  # noqa: E402

OUT = os.path.join(HERE, "..", "assets")
Q = 3

# Soft voices: a triangle (the console's round channel) with a little 25 %
# pulse under it for the chip edge, low-passed so nothing hisses.
TRI = Inst("triangle", adsr=(0.004, 0.09, 0.0, 0.03), cutoff=3200)
EDGE = Inst("square", duty=0.25, adsr=(0.003, 0.05, 0.0, 0.02), cutoff=1800, gain=0.18)
LEAD = Inst("square", duty=0.25, adsr=(0.006, 0.12, 0.35, 0.08), cutoff=2400, res=0.15, gain=0.6)


def note(name, dur, voices=(TRI, EDGE), vel=1.0):
    f = note_freq(name)
    return mix(*[v.render(f, dur, vel) for v in voices])


def seq(notes, step, dur, **kw):
    """Notes one after another, `step` seconds apart, each `dur` long."""
    parts = [note(n, dur, **kw) for n in notes]
    return mix(*parts, offsets=[i * step for i in range(len(parts))])


def main():
    os.makedirs(OUT, exist_ok=True)
    # Coin: a short grace note a fourth below, then the main note decaying.
    coin = mix(note("B5", 0.025, vel=0.6), note("E6", 0.1), offsets=[0.0, 0.028])
    save_ogg(os.path.join(OUT, "coin.ogg"), lowpass(coin, 3500), quality=Q)

    arc = seq(["B5", "E6", "F#6", "B6"], 0.045, 0.09)
    arc = echo(arc, time=0.09, feedback=0.25, wet=0.25)
    save_ogg(os.path.join(OUT, "arc.ogg"), lowpass(arc[: int(0.42 * 44100)], 3200), quality=Q)

    lead = (LEAD, Inst("triangle", adsr=(0.004, 0.15, 0.3, 0.08), cutoff=2500, gain=0.7))
    fan = mix(
        seq(["E5", "B5"], 0.11, 0.1, voices=lead),
        note("E6", 0.32, voices=lead),
        note("E4", 0.4, voices=(Inst("triangle", adsr=(0.004, 0.2, 0.4, 0.1), cutoff=1500),)),
        offsets=[0.0, 0.225, 0.225],
    )
    save_ogg(os.path.join(OUT, "milestone.ogg"), lowpass(fan, 3000), quality=Q)

    new = mix(note("B5", 0.06), note("B5", 0.06), note("E6", 0.16), offsets=[0.0, 0.09, 0.18])
    save_ogg(os.path.join(OUT, "new.ogg"), lowpass(new, 3200), quality=Q)


if __name__ == "__main__":
    main()
