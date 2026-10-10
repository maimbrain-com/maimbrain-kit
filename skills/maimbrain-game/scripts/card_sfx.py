"""The between-rounds card's sounds (ui::StatsCard, ui::CardSounds), made
with sfx.py: free, deterministic, a few KB each. Python stdlib plus `mb ogg`.

    python3 card_sfx.py games/<name>/assets/sfx            # writes card_*.ogg there
    python3 card_sfx.py games/<name>/assets/sfx --key A    # jingle and sting in A (default C)

Then `StatsCard::new(..).sounds(CardSounds::load("assets/sfx"))`. Put the
jingle and the sting in the key of the game's music (`mb music` logs it) so
they sit with it; they're short and bright, made to cover the fight music
dying away and the next fight's starting (UI.md "StatsCard").

    card_intro   a rising four-note jingle as the card comes in (~1.1 s)
    card_row     a soft swish as a row slides in
    card_tick    a tiny blip per step of a rolling number (the card pitches it up)
    card_land    a thunk with a ping as a number lands
    card_rank    the rank's stamp: a thump, a slap and a bell, with a little room
    card_teaser  a rising whoosh as the next opponent slides in
    card_wipe    a deep whoosh as the wipe closes
    card_outro   a sting into the next fight: a stab and a held chord (~1.4 s)
"""

from __future__ import annotations

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from sfx import Sfx, Song, mix, reverb, save_ogg, note_freq  # noqa: E402

SHIFT = {"C": 0, "C#": 1, "Db": 1, "D": 2, "D#": 3, "Eb": 3, "E": 4, "F": 5, "F#": 6, "Gb": 6, "G": 7, "G#": 8, "Ab": 8, "A": 9, "A#": 10, "Bb": 10, "B": 11}
NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]


def note(name: str, key: str) -> str:
    """`name` (written in C) moved into `key`."""
    n = name.rstrip("0123456789")
    octave = int(name[len(n):])
    semis = SHIFT[n] + SHIFT[key]
    return f"{NAMES[semis % 12]}{octave + semis // 12}"


def notes(pattern: str, key: str) -> str:
    return " ".join(t if t in (".", "-") or t.lower() == "x" else note(t, key) for t in pattern.split())


def main(out: str, key: str) -> None:
    os.makedirs(out, exist_ok=True)
    w = lambda name, s: save_ogg(os.path.join(out, f"card_{name}.ogg"), s, quality=3)  # noqa: E731

    intro = Song(bpm=168)
    intro.track("lead", notes("C5 E5 G5 C6 - - - - . . . .", key), vol=0.5)
    intro.track("arp", notes("C4 . G4 . C5 . . . . . . .", key), vol=0.25)
    intro.track("bass", notes("C3 - - - - - - - . . . .", key), vol=0.45)
    intro.track("hat", "x x x x X . . . . . . .", vol=0.2)
    w("intro", reverb(intro.render(bars=1, beats_per_bar=3, loop=False, tail=0.6), room=0.4, wet=0.18))

    w("row", Sfx("noise_burst", freq=3200, freq_end=1600, dur=0.09, vel=0.5).render())
    w("tick", Sfx("square", freq=1760, dur=0.022, vel=0.5).render())
    w("land", mix(Sfx("kick", freq=95, freq_end=60, dur=0.14).render(), Sfx("pluck", freq=note_freq(note("G5", key)), dur=0.12, vel=0.5).render()))
    stamp = mix(
        Sfx("kick", freq=70, freq_end=40, dur=0.35).render(),
        Sfx("snare", freq=190, dur=0.2, vel=0.8).render(),
        Sfx("bell", freq=note_freq(note("C6", key)), dur=0.6, vel=0.5).render(),
        offsets=[0.0, 0.0, 0.02],
    )
    w("rank", reverb(stamp, room=0.5, wet=0.2))
    w("teaser", Sfx("noise_burst", freq=700, freq_end=4200, dur=0.32, vel=0.6).render())
    w("wipe", mix(Sfx("noise_burst", freq=3600, freq_end=380, dur=0.5, vel=0.8).render(), Sfx("sine", freq=110, freq_end=45, dur=0.5, vel=0.6).render()))

    sting = Song(bpm=120)
    sting.track("lead", notes("G4 C5 - - - - - - - -", key), vol=0.5)
    sting.track("pad", notes(". C4 - - - - - - - -", key), vol=0.35)
    sting.track("pad", notes(". E4 - - - - - - - -", key), vol=0.3)
    sting.track("bass", notes("G2 C3 - - - - - - - -", key), vol=0.5)
    sting.track("snare", "X x . . . . . . . .", vol=0.4)
    w("outro", reverb(sting.render(bars=1, beats_per_bar=3, loop=False, tail=0.8), room=0.55, wet=0.22))


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    key = "C"
    if "--key" in sys.argv:
        key = sys.argv[sys.argv.index("--key") + 1]
        args = [a for a in args if a != key]
    if len(args) != 1 or key not in SHIFT:
        sys.exit(__doc__)
    main(args[0], key)
