# Run from the repo root: python3 games/studio-engine2/examples/lantern-path/placeholder_art.py games/studio-engine2/examples/lantern-path
# Lantern Path's art, drawn by this script (no generation): dusk layers and
# a moss tile set, procedurally (seeded, so the same every run), and a
# pixel-art fox (a walker sheet), lanterns and a firefly.
import math, random, sys
from PIL import Image, ImageDraw, ImageFilter

out = sys.argv[1]
rnd = random.Random(7)

def lerp(a, b, t): return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(len(a)))

def noise_line(w, base, amp, waves, seed):
    r = random.Random(seed)
    ph = [r.random() * 6.28 for _ in waves]
    return [base + sum(amp * a * math.sin(x / l * 6.28 + p) for (l, a), p in zip(waves, ph)) for x in range(w)]

W, H = 640, 360
# dusk_0: the sky, far: a gradient with a low sun glow and soft clouds.
sky = Image.new("RGBA", (W, H))
d = ImageDraw.Draw(sky)
top, mid, low = (22, 26, 68), (92, 52, 104), (238, 136, 92)
for y in range(H):
    t = y / (H - 1)
    c = lerp(top, mid, t / 0.6) if t < 0.6 else lerp(mid, low, (t - 0.6) / 0.4)
    d.line([(0, y), (W, y)], fill=c + (255,))
glow = Image.new("RGBA", (W, H), (0, 0, 0, 0))
g = ImageDraw.Draw(glow)
for r in range(120, 0, -4):
    a = int(90 * (1 - r / 120) ** 1.5)
    g.ellipse([W * 0.62 - r, H * 0.82 - r, W * 0.62 + r, H * 0.82 + r], fill=(255, 196, 120, a))
sky = Image.alpha_composite(sky, glow)
# The clouds wrap (the layer repeats as the camera runs): each is drawn
# three widths apart on a strip three widths wide, blurred there, and the
# middle width kept, so one running off the right comes back on the left.
clouds = Image.new("RGBA", (W * 3, H), (0, 0, 0, 0))
c = ImageDraw.Draw(clouds)
for i in range(14):
    x, y = rnd.randrange(0, W), rnd.randrange(40, 200)
    for k in range(6):
        rx, ry = rnd.randrange(30, 70), rnd.randrange(8, 16)
        dy = rnd.randrange(-4, 4)
        for copy in (0, W, 2 * W):
            c.ellipse([copy + x + k * 22 - rx, y - ry + dy, copy + x + k * 22 + rx, y + ry], fill=(180, 120, 150, 40))
clouds = clouds.filter(ImageFilter.GaussianBlur(6)).crop((W, 0, 2 * W, H))
sky = Image.alpha_composite(sky, clouds)
for i in range(60):  # first stars
    x, y = rnd.randrange(W), rnd.randrange(0, 120)
    sky.putpixel((x, y), (255, 240, 220, rnd.randrange(80, 200)))
sky.save(f"{out}/images/dusk_0.png")

def hills(name, color, base, amp, waves, seed, trees):
    img = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    line = noise_line(W, base, amp, waves, seed)
    # Wrap seamlessly: blend the ends.
    for x in range(W):
        t = x / W
        y = line[x] * (1 - t) + line[(x - W // 2) % W] * t if False else line[x]
        d.line([(x, int(line[x])), (x, H)], fill=color)
    r = random.Random(seed)
    for i in range(trees):
        x = r.randrange(W)
        y = line[x]
        h = r.randrange(30, 80)
        d.polygon([(x - h * 0.2, y + 4), (x + h * 0.2, y + 4), (x, y - h)], fill=color)
    img = img.filter(ImageFilter.GaussianBlur(0.8))
    img.save(f"{out}/images/{name}.png")

hills("dusk_1", (64, 40, 88, 255), 210, 18, [(640, 1.0), (213, 0.5), (128, 0.25)], 3, 26)
hills("dusk_2", (30, 22, 48, 255), 262, 14, [(320, 1.0), (160, 0.4), (64, 0.2)], 5, 14)

# The fox, the lanterns and the firefly: pixel art (below).

# Moss tiles, 32×32: moss (solid), ledge (one-way), thorns (hazard).
t = Image.new("RGBA", (96, 32), (0, 0, 0, 0))
d = ImageDraw.Draw(t)
r = random.Random(11)
soil, moss, moss2 = (54, 38, 46, 255), (86, 140, 76, 255), (120, 176, 92, 255)
d.rectangle([0, 0, 31, 31], fill=soil)
for i in range(40):
    x, y = r.randrange(32), r.randrange(10, 32)
    d.point((x, y), fill=(70, 50, 58, 255))
d.rectangle([0, 0, 31, 7], fill=moss)
for x in range(0, 32, 3):
    d.line([(x, 7), (x + 1, 9 + r.randrange(3))], fill=moss)
    d.point((x + 1, 1 + r.randrange(4)), fill=moss2)
d.rectangle([34, 2, 61, 10], fill=(110, 74, 56, 255))      # ledge: a branch
d.rectangle([32, 0, 63, 4], fill=moss)
d.line([(40, 10), (38, 16)], fill=(110, 74, 56, 255), width=2)
for k in range(5):                                          # thorns
    x = 64 + k * 6 + 2
    d.polygon([(x, 31), (x + 3, 14 + r.randrange(6)), (x + 6, 31)], fill=(150, 40, 70, 255))
    d.polygon([(x + 2, 31), (x + 3, 20), (x + 4, 31)], fill=(210, 80, 100, 255))
t.save(f"{out}/tiles/moss.png")

# --- Pixel art, by hand: a palette with a dark outline, shapes on a grid,
# then an outline around them. The fox is a walker sheet (body, then one
# paw); at its in-game height (46) each pixel is 2 units.

PAL = {
    "K": (42, 22, 34, 255),     # outline
    "O": (232, 118, 44, 255),   # orange
    "D": (176, 78, 34, 255),    # shade
    "H": (250, 160, 80, 255),   # highlight
    "C": (248, 228, 196, 255),  # cream
    "W": (255, 250, 240, 255),  # white tip
    "L": (58, 34, 40, 255),     # legs, paws
    "E": (24, 12, 18, 255),     # eye, nose
}

def grid(w, h):
    return [[None] * w for _ in range(h)]

def ell(g, cx, cy, rx, ry, c, test=None):
    for y in range(len(g)):
        for x in range(len(g[0])):
            if ((x + 0.5 - cx) / rx) ** 2 + ((y + 0.5 - cy) / ry) ** 2 <= 1.0 and (test is None or test(x, y)):
                g[y][x] = c

def tri(g, a, b, c, col):
    def side(p, q, r):
        return (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
    for y in range(len(g)):
        for x in range(len(g[0])):
            p = (x + 0.5, y + 0.5)
            s1, s2, s3 = side(a, b, p), side(b, c, p), side(c, a, p)
            if (s1 >= 0 and s2 >= 0 and s3 >= 0) or (s1 <= 0 and s2 <= 0 and s3 <= 0):
                g[y][x] = col

def rect(g, x0, y0, x1, y1, c):
    for y in range(y0, y1 + 1):
        for x in range(x0, x1 + 1):
            g[y][x] = c

def outline(g):
    h, w = len(g), len(g[0])
    out = [row[:] for row in g]
    for y in range(h):
        for x in range(w):
            if g[y][x] is None and any(0 <= y + dy < h and 0 <= x + dx < w and g[y + dy][x + dx] not in (None, "K") for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1))):
                out[y][x] = "K"
    return out

def paint(img, g, ox, oy, k=1):
    for y, row in enumerate(g):
        for x, c in enumerate(row):
            if c:
                for j in range(k):
                    for i in range(k):
                        img.putpixel((ox + x * k + i, oy + y * k + j), PAL[c] if isinstance(c, str) else c)

def fox():
    # Body cell: 34 × 24 (a 1-pixel margin for the outline). Facing right.
    g = grid(34, 24)
    # Tail: a bushy plume behind, sweeping up, white at the tip.
    for cx, cy, r in ((10.5, 13.5, 3.6), (8, 12, 3.6), (5.8, 10, 3.4), (4.2, 7.6, 3.0)):
        ell(g, cx, cy, r, r, "O")
    for cx, cy, r in ((9.5, 15, 2.6), (7, 13.6, 2.4)):
        ell(g, cx, cy, r, r * 0.8, "D")
    ell(g, 3.6, 6.4, 2.4, 2.4, "W")
    # Body: orange, shaded underneath.
    ell(g, 17.5, 13.5, 7.6, 4.6, "O")
    ell(g, 17, 16, 6.5, 2.0, "D")
    rect(g, 13, 10, 19, 10, "H")
    # Dark legs (the paws are the second cell, drawn under them).
    rect(g, 12, 16, 13, 19, "L")
    rect(g, 21, 16, 22, 19, "L")
    # Chest: cream.
    ell(g, 23.5, 12.5, 2.6, 3.2, "C")
    # Head, cheek and pointed snout.
    ell(g, 26, 8, 4.6, 3.6, "O")
    ell(g, 27.5, 9.6, 3.0, 1.6, "C")
    tri(g, (28, 6.2), (33, 8.6), (28, 10.4), "O")
    tri(g, (28, 8.6), (33, 8.6), (28, 10.4), "C")
    g[8][32] = "E"
    # Ears: two points, dark inside.
    tri(g, (21.2, 6.5), (23, 0.5), (25, 5.5), "O")
    tri(g, (25.2, 5.2), (27.6, 0.5), (29.2, 6.2), "O")
    tri(g, (22.6, 5.6), (23.2, 2.4), (24.2, 5.2), "D")
    tri(g, (26.6, 5.2), (27.6, 2.4), (28.4, 5.6), "D")
    g[7][27] = "E"
    g = outline(g)
    # Foot cell: a small dark paw.
    f = grid(34, 24)
    ell(f, 17, 21, 3.2, 1.2, "L")
    f[21][18] = "D"
    f = outline(f)
    img = Image.new("RGBA", (68, 24), (0, 0, 0, 0))
    paint(img, g, 0, 0)
    paint(img, f, 34, 0)
    return img

def lantern():
    # Two 16 × 24 frames, dark then lit, drawn twice as big (32 × 48 each).
    def frame(lit):
        g = grid(16, 24)
        iron, iron2 = (52, 46, 62, 255), (84, 76, 96, 255)
        glass = (255, 214, 120, 255) if lit else (66, 74, 104, 255)
        glass2 = (255, 244, 196, 255) if lit else (92, 102, 136, 255)
        for y in range(1, 4):
            g[y][7] = iron
        g[0][6] = g[0][8] = iron
        rect(g, 4, 4, 11, 5, iron)
        rect(g, 5, 6, 10, 15, glass)
        rect(g, 6, 7, 7, 12, glass2)
        for y in range(6, 16):
            g[y][4] = g[y][11] = iron
            g[y][8] = iron2
        rect(g, 3, 16, 12, 17, iron)
        rect(g, 5, 18, 10, 18, iron2)
        return outline(g), lit
    img = Image.new("RGBA", (64, 48), (0, 0, 0, 0))
    for i, lit in enumerate((False, True)):
        g, _ = frame(lit)
        if lit:
            halo = Image.new("RGBA", (32, 48), (0, 0, 0, 0))
            for r, a in ((7, 40), (6, 70), (5, 100)):
                for y in range(24):
                    for x in range(16):
                        if (x + 0.5 - 8) ** 2 + (y + 0.5 - 11) ** 2 <= r * r:
                            for j in range(2):
                                for k in range(2):
                                    halo.putpixel((x * 2 + k, y * 2 + j), (255, 200, 110, a))
            img.alpha_composite(halo, (32, 0))
        paint(img, g, i * 32, 0, 2)
    return img

def firefly():
    # A little bug with a glowing tail and a soft halo (8 × 8, drawn twice as big).
    g = grid(8, 8)
    for y in range(8):
        for x in range(8):
            d = ((x + 0.5 - 4) ** 2 + (y + 0.5 - 4.5) ** 2) ** 0.5
            if d <= 3.6:
                g[y][x] = (255, 214, 110, 70 if d > 2.6 else 130)
    for x, y in ((3, 4), (4, 4), (3, 5), (4, 5)):
        g[y][x] = (255, 246, 170, 255)
    g[3][3] = g[3][4] = (64, 44, 40, 255)          # the body
    g[2][2] = g[2][5] = (220, 230, 255, 170)        # wings
    g[1][2] = g[1][5] = (220, 230, 255, 110)
    img = Image.new("RGBA", (16, 16), (0, 0, 0, 0))
    paint(img, g, 0, 0, 2)
    return img

fox().save(f"{out}/images/fox.png")
lantern().save(f"{out}/images/lantern.png")
firefly().save(f"{out}/images/firefly.png")
print("ok")
