#!/usr/bin/env python3
"""Draws the retro scene's pixel art (scene=retro, src/retro.rs): a banded
dusk sky, far mountains, near hills and a tiling ground for the Mode 7
floor, SNES-style (few colors, hard pixels, a little ordered dither).

    python3 games/mb3d-demo/tools/make_retro_art.py

Deterministic (seeded), free; writes PNGs into games/mb3d-demo/assets/.
"""
import math
import os
import random

from PIL import Image

OUT = os.path.join(os.path.dirname(__file__), "..", "assets")
BAYER = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]]


def hexc(h, a=255):
    return ((h >> 16) & 255, (h >> 8) & 255, h & 255, a)


def sky():
    # 256 × 96: bands from deep violet at the top to a hot orange horizon, dithered between bands.
    bands = [0x1B1446, 0x2A1A5E, 0x46206E, 0x6B2A72, 0x983670, 0xC4486A, 0xE86A5A, 0xF89A58, 0xFCC468]
    w, h = 256, 96
    img = Image.new("RGBA", (w, h))
    for y in range(h):
        t = y / (h - 1) * (len(bands) - 1)
        i = min(int(t), len(bands) - 2)
        f = t - i
        for x in range(w):
            pick = bands[i + 1] if f * 16 > BAYER[y % 4][x % 4] + 0.5 else bands[i]
            img.putpixel((x, y), hexc(pick))
    img.save(os.path.join(OUT, "retro_sky.png"))


def ridge(name, w, h, base, peaks, rough, colors, seed, snow=None):
    rnd = random.Random(seed)
    phases = [rnd.random() * math.tau for _ in range(4)]
    img = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    for x in range(w):
        u = x / w * math.tau
        top = base
        top += peaks * (0.5 + 0.5 * math.sin(u * 3 + phases[0])) * (0.6 + 0.4 * math.sin(u * 7 + phases[1]))
        top += rough * math.sin(u * 17 + phases[2]) + rough * 0.5 * math.sin(u * 31 + phases[3])
        top = int(h - top)
        for y in range(max(top, 0), h):
            d = y - top
            c = colors[0] if d < 2 else colors[1] if d < 6 else colors[2]
            # Light from the left on the slopes: lighter where the ridge climbs.
            if d >= 2 and ((x * 7 + y * 3) % 11 == 0):
                c = colors[0]
            if snow is not None and d < 3 and top < h - base - peaks * 0.7:
                c = snow
            img.putpixel((x, y), hexc(c))
    img.save(os.path.join(OUT, name))


def ground():
    # 32 × 32: two grass tones in a checker of 16-pixel tiles, with tufts and a path seam.
    a, b, dark, light = 0x3E8948, 0x2F6F3E, 0x265C33, 0x63AB3F
    rnd = random.Random(7)
    img = Image.new("RGBA", (32, 32))
    for y in range(32):
        for x in range(32):
            c = a if ((x // 16) + (y // 16)) % 2 == 0 else b
            img.putpixel((x, y), hexc(c))
    for _ in range(26):
        x, y = rnd.randrange(32), rnd.randrange(31)
        img.putpixel((x, y), hexc(light))
        img.putpixel((x, (y + 1) % 32), hexc(dark))
    img.save(os.path.join(OUT, "retro_ground.png"))


if __name__ == "__main__":
    sky()
    ridge("retro_mountains.png", 256, 64, 10, 40, 3, [0x8E6BA8, 0x5C4A86, 0x3D3268], seed=3, snow=0xE8E0F0)
    ridge("retro_hills.png", 256, 40, 6, 18, 2, [0x4F9A5A, 0x2E6E4A, 0x1F4E3C], seed=11)
    ground()
    print("wrote", sorted(f for f in os.listdir(OUT) if f.startswith("retro_")))
