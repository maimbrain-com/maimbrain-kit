#!/usr/bin/env python3
"""Draws the steel deck for the hero scene's grazing view (scene=hero
view=grazing, src/hero.rs): a 256² tiling plate with diamond tread, panel
seams and rivets, fine detail that blurs at a slant without anisotropic
filtering. python3 tools/make_deck_plate.py assets/deck_plate.jpg"""
import sys
import numpy as np
from PIL import Image

N = 256
y, x = np.mgrid[0:N, 0:N].astype(np.float32)
rng = np.random.default_rng(7)


def blur(a, n):
    for _ in range(n):
        a = (a + np.roll(a, 1, 0) + np.roll(a, -1, 0) + np.roll(a, 1, 1) + np.roll(a, -1, 1)) / 5
    return a


base = 0.42 + blur(0.05 * rng.standard_normal((N, N)).astype(np.float32), 2)
cell = 16.0
u = (x % cell) / cell - 0.5
v = (y % cell) / cell - 0.5
alt = ((np.floor(x / cell) + np.floor(y / cell)) % 2) * 2 - 1
a = np.where(alt > 0, u + v, u - v)
b = np.where(alt > 0, u - v, u + v)
lozenge = (np.abs(a) < 0.32) & (np.abs(b) < 0.07)
c = base.copy()
c[lozenge] += 0.28
edge = (np.abs(a) < 0.34) & (np.abs(b) < 0.10) & ~lozenge
c[edge] -= 0.18
seam = (np.minimum(x % 128, 128 - x % 128) < 1.5) | (np.minimum(y % 128, 128 - y % 128) < 1.5)
c[seam] = 0.08
for cx in range(8, 256, 16):
    for cy in (6, 122, 134, 250):
        for (px, py) in ((cx, cy), (cy, cx)):
            d = np.hypot((x - px + 128) % 256 - 128, (y - py + 128) % 256 - 128)
            c[d < 2.5] = 0.75
            c[(d >= 2.5) & (d < 3.5)] = 0.15
c = np.clip(c, 0, 1)
rgb = np.stack([c * 0.92, c * 0.95, c * 1.0], -1)
rust = blur(np.clip(rng.standard_normal((N, N)) * 0.5, 0, 1).astype(np.float32), 4)
rgb[..., 0] += rust * 0.10
rgb[..., 2] -= rust * 0.06
Image.fromarray((np.clip(rgb, 0, 1) * 255).astype(np.uint8)).save(sys.argv[1], quality=90)
