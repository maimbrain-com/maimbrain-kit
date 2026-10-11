"""The template's placeholder effects, drawn in code with sfx.py (beside this
file): assets/pop.ogg, miss.ogg and over.ogg. Make the game's own sounds
(here, or `mb sfx`; the skill's assets.md) and keep the names src/sound.rs
plays, or change its FILES.

    python3 tools/sounds.py          # from the game's directory
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from sfx import Sfx, mix, noise, reverb, save_ogg  # noqa: E402

out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets")
os.makedirs(out, exist_ok=True)

# A bright bubble pop: a quick upward blip over a tiny burst of air.
pop = mix(Sfx("pluck", freq=880, freq_end=1760, dur=0.07).render(), noise(0.05, cutoff=6000, decay=0.015, seed=3), gains=[1.0, 0.4])
# A soft dull tap: the finger missed.
miss = Sfx("triangle", freq=330, freq_end=240, dur=0.06, adsr=(0.002, 0.05, 0.0, 0.03)).render()
# The bubble's gone: three falling notes.
over = reverb(mix(*(Sfx("square", freq=f, dur=0.16, duty=0.25, cutoff=2500).render() for f in (523.25, 415.3, 329.63)), offsets=[0.0, 0.17, 0.34]), room=0.4, wet=0.2)

for name, x in (("pop", pop), ("miss", miss), ("over", over)):
    save_ogg(os.path.join(out, f"{name}.ogg"), x)
    print(f"assets/{name}.ogg")
