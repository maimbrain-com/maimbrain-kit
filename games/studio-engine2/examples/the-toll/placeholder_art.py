# Run from the repo root (Pillow): python3 games/studio-engine2/examples/the-toll/placeholder_art.py games/studio-engine2/examples/the-toll
# The Toll's art, drawn by this script (no generation; seeded, so the same
# every run): the bridge at dusk as a painting, Gruk on his bridge and Pip
# as pixel art, the toll gate's two frames, and the two talking portraits
# (`generate(kind: "portrait")`'s layout: four mouths across, a row per
# expression, the blink row last; 48 art pixels a cell, 4 sheet pixels each).
import math, random, sys
from PIL import Image, ImageDraw, ImageFilter

out = sys.argv[1]
rnd = random.Random(11)


def lerp(a, b, t):
    t = max(0.0, min(1.0, t))
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(len(a)))


def hexc(s, a=255):
    s = s.lstrip("#")
    return (int(s[0:2], 16), int(s[2:4], 16), int(s[4:6], 16), a)


# ---------------------------------------------------------------------------
# Pixel art helpers: draw on a small canvas, outline, scale up nearest.

class Px:
    def __init__(self, w, h):
        self.w, self.h = w, h
        self.img = Image.new("RGBA", (w, h), (0, 0, 0, 0))
        self.p = self.img.load()

    def set(self, x, y, c):
        x, y = int(x), int(y)
        if 0 <= x < self.w and 0 <= y < self.h:
            self.p[x, y] = c

    def get(self, x, y):
        if 0 <= x < self.w and 0 <= y < self.h:
            return self.p[x, y]
        return (0, 0, 0, 0)

    def ellipse(self, cx, cy, rx, ry, c):
        for y in range(int(cy - ry - 1), int(cy + ry + 2)):
            for x in range(int(cx - rx - 1), int(cx + rx + 2)):
                if ((x + 0.5 - cx) / rx) ** 2 + ((y + 0.5 - cy) / ry) ** 2 <= 1.0:
                    self.set(x, y, c)

    def rect(self, x0, y0, x1, y1, c):
        for y in range(int(y0), int(y1)):
            for x in range(int(x0), int(x1)):
                self.set(x, y, c)

    def poly(self, pts, c):
        xs = [p[0] for p in pts]
        ys = [p[1] for p in pts]
        for y in range(int(min(ys)), int(max(ys)) + 1):
            for x in range(int(min(xs)), int(max(xs)) + 1):
                if inside(pts, x + 0.5, y + 0.5):
                    self.set(x, y, c)

    def line(self, pts, c):
        for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
            n = int(max(abs(x1 - x0), abs(y1 - y0))) + 1
            for i in range(n + 1):
                t = i / max(n, 1)
                self.set(round(x0 + (x1 - x0) * t), round(y0 + (y1 - y0) * t), c)

    def shade(self, f, c):
        # Recolour pixels that are already drawn where f(x, y) holds.
        for y in range(self.h):
            for x in range(self.w):
                if self.p[x, y][3] and f(x, y):
                    self.p[x, y] = c

    def outline(self, c, below=None):
        src = self.img.copy().load()
        for y in range(self.h):
            for x in range(self.w):
                if src[x, y][3]:
                    continue
                if below is not None and y >= below:
                    continue
                for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                    xx, yy = x + dx, y + dy
                    if 0 <= xx < self.w and 0 <= yy < self.h and src[xx, yy][3]:
                        self.p[x, y] = c
                        break

    def scaled(self, k):
        return self.img.resize((self.w * k, self.h * k), Image.NEAREST)


def inside(pts, x, y):
    n, c = len(pts), False
    j = n - 1
    for i in range(n):
        xi, yi = pts[i]
        xj, yj = pts[j]
        if (yi > y) != (yj > y) and x < (xj - xi) * (y - yi) / (yj - yi + 1e-9) + xi:
            c = not c
        j = i
    return c


INK = hexc("#22161c")

# ---------------------------------------------------------------------------
# Gruk's portrait: a mossy old bridge troll.

G = {
    "skin": hexc("#7d9a5a"), "skin_hi": hexc("#9cb874"), "skin_lo": hexc("#5a7444"), "skin_dk": hexc("#3f5636"),
    "nose": hexc("#8fa860"), "wart": hexc("#6b7f3f"), "tusk": hexc("#efe6c8"), "tusk_lo": hexc("#c9bb93"),
    "mouth": hexc("#3a1e22"), "tongue": hexc("#b5485a"), "teeth": hexc("#f2ecd6"),
    "eye": hexc("#f7e7a6"), "pupil": hexc("#2a1a14"), "brow": hexc("#3b3a2a"),
    "hair": hexc("#5f6e3a"), "hair_hi": hexc("#7f8e48"), "vest": hexc("#7b4f2e"), "vest_hi": hexc("#9a6a3f"),
    "shirt": hexc("#b7a37a"), "strap": hexc("#4b3020"), "cheek": hexc("#a47e62"),
}


def gruk_face(expr, mouth, blink):
    c = G
    s = Px(48, 48)
    # Shoulders and vest.
    s.ellipse(24, 52, 23, 14, c["vest"])
    s.poly([(17, 39), (31, 39), (28, 48), (20, 48)], c["shirt"])
    s.line([(14, 40), (19, 48)], c["strap"])
    s.shade(lambda x, y: y >= 40 and (x < 8 or x > 40) and s.get(x, y) == c["vest"], c["vest_hi"])
    # Ears.
    s.poly([(7, 20), (1, 13), (3, 24), (9, 27)], c["skin_lo"])
    s.poly([(41, 20), (47, 13), (45, 24), (39, 27)], c["skin_lo"])
    s.set(3, 16, c["skin_dk"]); s.set(44, 16, c["skin_dk"])
    # Head: a broad dome on a wide jaw.
    s.ellipse(24, 19, 16, 14, c["skin"])
    s.ellipse(24, 30, 18, 11, c["skin"])
    # Light from the upper left, shadow under the jaw.
    s.shade(lambda x, y: (x - 17) ** 2 + (y - 11) ** 2 < 40, c["skin_hi"])
    s.shade(lambda x, y: y >= 36 or (x > 36 and y > 22), c["skin_lo"])
    # Moss on top.
    for x, h in ((15, 3), (17, 5), (19, 4), (21, 6), (23, 5), (25, 6), (27, 4), (29, 5), (31, 3), (33, 2)):
        s.rect(x, 7 - h + 3, x + 2, 9, c["hair"])
        s.set(x, 7 - h + 3, c["hair_hi"])
    # Warts and a scar.
    s.set(33, 27, c["wart"]); s.set(34, 27, c["wart"]); s.set(12, 31, c["wart"])
    s.line([(30, 12), (33, 16)], c["skin_lo"])
    # Brows (heavy) and eyes.
    ey = 19
    if blink or expr == "laughing":
        for ex in (17, 31):
            if expr == "laughing":
                s.line([(ex - 3, ey + 1), (ex - 1, ey - 1), (ex + 1, ey - 1), (ex + 3, ey + 1)], INK)
            else:
                s.line([(ex - 3, ey), (ex + 3, ey)], INK)
    else:
        for ex in (17, 31):
            s.ellipse(ex, ey, 3.2, 2.6, c["eye"])
            px = ex + (1 if expr == "smug" else 0)
            s.rect(px - 1, ey - 1, px + 1, ey + 2, c["pupil"])
            if expr in ("bored", "smug", "neutral"):
                # Heavy lids.
                lid = 2 if expr == "bored" else (1 if expr == "neutral" else 2)
                s.rect(ex - 3, ey - 3, ex + 4, ey - 3 + lid + 1, c["skin_lo"])
                s.line([(ex - 3, ey - 3 + lid + 1), (ex + 3, ey - 3 + lid + 1)], INK)
            if expr == "annoyed":
                s.rect(ex - 3, ey - 3, ex + 4, ey - 1, c["skin_lo"])
    # Brows by expression: (left inner y, left outer y, right inner, right outer).
    by = {"neutral": (15, 15, 15, 15), "smug": (15, 15, 15, 12), "annoyed": (17, 14, 17, 14), "laughing": (13, 14, 13, 14),
          "bored": (16, 16, 16, 16)}.get(expr, (15, 15, 15, 15))
    s.line([(21, by[0]), (13, by[1])], c["brow"]); s.line([(21, by[0] - 1), (13, by[1] - 1)], c["brow"])
    s.line([(27, by[2]), (35, by[3])], c["brow"]); s.line([(27, by[2] - 1), (35, by[3] - 1)], c["brow"])
    # A big nose.
    s.ellipse(24, 25, 4.2, 3.4, c["nose"])
    s.set(22, 26, c["skin_dk"]); s.set(26, 26, c["skin_dk"]); s.set(23, 23, c["skin_hi"])
    if expr == "laughing":
        s.ellipse(14, 28, 2, 1, c["cheek"]); s.ellipse(34, 28, 2, 1, c["cheek"])
    # Mouth: four openings; a smirk pulls the right corner up.
    my = 32
    if expr == "laughing":
        mouth = max(mouth, 2)
    if mouth == 0:
        if expr == "smug":
            s.line([(16, my), (26, my), (31, my - 2)], c["mouth"])
        elif expr in ("annoyed", "bored"):
            s.line([(16, my + 1), (31, my + 1)], c["mouth"])
        else:
            s.line([(16, my), (31, my)], c["mouth"])
    else:
        h = {1: 2, 2: 4, 3: 6}[mouth]
        w = {1: 7, 2: 8, 3: 9}[mouth]
        s.ellipse(24, my + h / 2 - 0.5, w, h / 2 + 0.6, c["mouth"])
        if h >= 4:
            s.ellipse(24, my + h - 1, w - 3, 1.4, c["tongue"])
        # Upper teeth: a crooked row.
        for x in range(24 - w + 3, 24 + w - 2, 2):
            s.set(x, my - 1 + (1 if h > 2 else 0), c["teeth"])
        if expr == "smug":
            s.set(24 + w, my - 1, c["mouth"])
    # Tusks over the upper lip (an underbite).
    for tx in (17, 30):
        top = my - 4 - (1 if mouth >= 2 else 0)
        s.rect(tx, top, tx + 2, my + 1, c["tusk"])
        s.set(tx + 1, top, c["tusk_lo"]); s.set(tx, my, c["tusk_lo"])
    s.outline(INK, below=46)
    return s


P = {
    "skin": hexc("#f2c9a2"), "skin_hi": hexc("#fbe0c4"), "skin_lo": hexc("#d9a37e"),
    "hood": hexc("#3f6fa8"), "hood_hi": hexc("#5b8fcc"), "hood_lo": hexc("#2c4f7c"), "lining": hexc("#e3c25a"),
    "hair": hexc("#7a4426"), "hair_hi": hexc("#9b5a33"), "eye": hexc("#ffffff"), "iris": hexc("#3b5a8f"), "pupil": hexc("#1b1626"),
    "mouth": hexc("#5a2430"), "tongue": hexc("#d4677a"), "cheek": hexc("#f09a8a"), "scarf": hexc("#c8463c"), "scarf_hi": hexc("#e2665a"),
    "sweat": hexc("#bfe6ff"), "brow": hexc("#5e321c"),
}


def pip_face(expr, mouth, blink):
    c = P
    s = Px(48, 48)
    # Cloak and scarf.
    s.ellipse(24, 53, 20, 13, c["hood"])
    s.shade(lambda x, y: x > 30, c["hood_lo"])
    s.ellipse(24, 42, 12, 4, c["scarf"])
    s.rect(28, 42, 33, 50, c["scarf"])
    s.shade(lambda x, y: y <= 41 and s.get(x, y) == c["scarf"], c["scarf_hi"])
    # Hood around the face, pointed at the top.
    s.ellipse(24, 23, 17, 17, c["hood"])
    s.poly([(18, 9), (24, 0), (30, 9)], c["hood"])
    s.shade(lambda x, y: (x - 17) ** 2 + (y - 14) ** 2 < 30 and s.get(x, y) == c["hood"], c["hood_hi"])
    s.shade(lambda x, y: x > 33 and s.get(x, y) == c["hood"], c["hood_lo"])
    s.ellipse(24, 26, 13, 13, c["lining"])
    # Face, round, with a fringe.
    s.ellipse(24, 27, 11.5, 11.5, c["skin"])
    s.shade(lambda x, y: (x - 19) ** 2 + (y - 22) ** 2 < 18 and s.get(x, y) == c["skin"], c["skin_hi"])
    s.shade(lambda x, y: y > 34 and s.get(x, y) == c["skin"], c["skin_lo"])
    for x0 in range(14, 35, 3):
        s.poly([(x0, 15), (x0 + 4, 15), (x0 + 2, 20 + (x0 % 2))], c["hair"])
    s.rect(14, 15, 35, 17, c["hair"])
    s.set(18, 16, c["hair_hi"]); s.set(25, 16, c["hair_hi"])
    # Big eyes.
    ey = 26
    for ex in (19, 29):
        if blink:
            s.line([(ex - 3, ey + 1), (ex + 3, ey + 1)], INK)
        elif expr == "happy":
            s.line([(ex - 3, ey + 1), (ex - 1, ey - 1), (ex + 1, ey - 1), (ex + 3, ey + 1)], INK)
        else:
            s.ellipse(ex, ey, 3.4, 3.9, c["eye"])
            s.ellipse(ex + (0 if expr != "nervous" else -1), ey + 0.5, 2.2, 2.6, c["iris"])
            s.rect(ex - 1 + (0 if expr != "nervous" else -1), ey, ex + 1 + (0 if expr != "nervous" else -1), ey + 2, c["pupil"])
            s.set(ex - 1, ey - 2, c["eye"]); s.set(ex, ey - 2, c["eye"])
    # Brows.
    if expr == "nervous":
        s.line([(16, 21), (21, 20)], c["brow"]); s.line([(27, 20), (32, 21)], c["brow"])
        s.ellipse(35, 22, 1.4, 2.2, c["sweat"]); s.set(35, 20, c["sweat"])
    elif expr == "happy":
        s.line([(16, 21), (21, 21)], c["brow"]); s.line([(27, 21), (32, 21)], c["brow"])
    else:
        s.line([(16, 22), (21, 21)], c["brow"]); s.line([(27, 21), (32, 22)], c["brow"])
    s.ellipse(15, 31, 2, 1.2, c["cheek"]); s.ellipse(33, 31, 2, 1.2, c["cheek"])
    s.set(24, 30, c["skin_lo"])
    # Mouth.
    my = 34
    if mouth == 0:
        if expr == "happy":
            s.line([(21, my - 1), (22, my), (26, my), (27, my - 1)], c["mouth"])
        elif expr == "nervous":
            s.line([(21, my), (22, my - 1), (23, my), (24, my - 1), (25, my), (26, my - 1), (27, my)], c["mouth"])
        else:
            s.line([(22, my), (26, my)], c["mouth"])
    else:
        h = {1: 1.4, 2: 2.4, 3: 3.4}[mouth]
        w = {1: 2.6, 2: 3.2, 3: 3.8}[mouth]
        s.ellipse(24, my + h / 2, w, h, c["mouth"])
        if mouth >= 2:
            s.ellipse(24, my + h, w - 1.2, 1, c["tongue"])
    s.outline(INK, below=46)
    return s


def sheet(face, rows, name):
    k = 4
    img = Image.new("RGBA", (48 * k * 4, 48 * k * len(rows)), (0, 0, 0, 0))
    for r, expr in enumerate(rows):
        for m in range(4):
            if expr == "blink":
                f = face("neutral", m, True)
            else:
                f = face(expr, m, False)
            img.paste(f.scaled(k), (m * 48 * k, r * 48 * k))
    img.save(f"{out}/images/portraits/{name}.png")
    rows_s = ", ".join(f'"{r}"' for r in rows)
    with open(f"{out}/images/portraits/{name}.toml", "w") as f:
        f.write(f"# A talking portrait (generate kind \"portrait\"'s layout): four mouths across,\n# a row per expression, the blink row last.\nframe = [{48 * k}, {48 * k}]\nportrait = [{rows_s}]\n")


sheet(gruk_face, ["neutral", "smug", "annoyed", "laughing", "bored", "blink"], "gruk")
sheet(pip_face, ["neutral", "nervous", "happy", "blink"], "pip")

# ---------------------------------------------------------------------------
# The bridge at dusk: a painting at 2 pixels a unit (720×1280 for the
# 360×640 canvas), drawn at scale 0.5.

W, H = 720, 1280
bg = Image.new("RGBA", (W, H))
d = ImageDraw.Draw(bg)
top, mid, low = hexc("#2d2b5e"), hexc("#a65d7c"), hexc("#f6b26b")
for y in range(H):
    t = y / 760
    col = lerp(top, mid, t / 0.55) if t < 0.55 else lerp(mid, low, (t - 0.55) / 0.45)
    d.line([(0, y), (W, y)], fill=col)
# The sun low behind the mountains, a soft glow.
glow = Image.new("RGBA", (W, H), (0, 0, 0, 0))
g = ImageDraw.Draw(glow)
for r in range(260, 0, -6):
    a = int(120 * (1 - r / 260) ** 1.8)
    g.ellipse([470 - r, 640 - r, 470 + r, 640 + r], fill=(255, 214, 150, a))
g.ellipse([440, 610, 500, 670], fill=(255, 236, 190, 255))
bg = Image.alpha_composite(bg, glow)
# Clouds, soft streaks.
cl = Image.new("RGBA", (W, H), (0, 0, 0, 0))
c = ImageDraw.Draw(cl)
for i in range(16):
    x, y = rnd.randrange(-100, W), rnd.randrange(120, 560)
    for k in range(5):
        rx, ry = rnd.randrange(50, 120), rnd.randrange(8, 18)
        c.ellipse([x + k * 40 - rx, y - ry, x + k * 40 + rx, y + ry], fill=(255, 190, 170, 34))
bg = Image.alpha_composite(bg, cl.filter(ImageFilter.GaussianBlur(8)))
for i in range(70):
    bg.putpixel((rnd.randrange(W), rnd.randrange(0, 300)), (255, 245, 230, rnd.randrange(90, 220)))


def ridge(base, amp, waves, seed, color, blur=0, snow=None):
    r = random.Random(seed)
    ph = [r.random() * 6.28 for _ in waves]
    layer = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    dd = ImageDraw.Draw(layer)
    pts = [(x, base + sum(amp * a * math.sin(x / l * 6.28 + p) for (l, a), p in zip(waves, ph))) for x in range(0, W + 4, 4)]
    dd.polygon(pts + [(W, H), (0, H)], fill=color)
    if snow:
        for x, y in pts:
            if y < base - amp * 0.5:
                dd.line([(x, y), (x, y + 10)], fill=snow)
    return layer.filter(ImageFilter.GaussianBlur(blur)) if blur else layer


bg = Image.alpha_composite(bg, ridge(600, 70, [(300, 1.0), (120, 0.4), (55, 0.15)], 1, hexc("#6d5b8f"), 2, hexc("#c9b6d9", 160)))
bg = Image.alpha_composite(bg, ridge(690, 50, [(220, 1.0), (90, 0.35), (40, 0.1)], 2, hexc("#4c4470"), 1))
# A lake under the mountains, the sunset in it, and a shimmer of the sun.
lake = Image.new("RGBA", (W, H), (0, 0, 0, 0))
gd = ImageDraw.Draw(lake)
for y in range(770, H):
    t = (y - 770) / 330
    gd.line([(0, y), (W, y)], fill=lerp(hexc("#e7a274"), hexc("#3b4f7a"), t))
for y in range(780, 1040, 3):
    w = 60 * (1 - (y - 780) / 300) + 10
    gd.line([(470 - w, y), (470 + w, y)], fill=(255, 226, 170, 110))
for i in range(70):
    y = rnd.randrange(780, 1040)
    x = rnd.randrange(0, W)
    gd.line([(x, y), (x + rnd.randrange(14, 60), y)], fill=(255, 236, 210, 70))
bg = Image.alpha_composite(bg, lake.filter(ImageFilter.GaussianBlur(1)))
# Pine-dark shores on either side of the lake, a line of trees on each.
shore = Image.new("RGBA", (W, H), (0, 0, 0, 0))
sd = ImageDraw.Draw(shore)
sd.polygon([(0, 740), (120, 760), (230, 800), (300, 830), (0, 860)], fill=hexc("#2e3a4a"))
sd.polygon([(W, 720), (600, 750), (520, 790), (460, 815), (W, 850)], fill=hexc("#2a3546"))
for x0, x1, base, k in ((0, 280, 790, 1), (470, W, 780, -1)):
    for x in range(x0, x1, 14):
        hgt = rnd.randrange(30, 70)
        y = base + (x - x0) * 0.12 * k + rnd.randrange(-6, 6)
        sd.polygon([(x - 9, y), (x, y - hgt), (x + 9, y)], fill=hexc("#253041"))
bg = Image.alpha_composite(bg, shore.filter(ImageFilter.GaussianBlur(0.8)))
cliff = Image.new("RGBA", (W, H), (0, 0, 0, 0))
cd = ImageDraw.Draw(cliff)
lp = [(0, 1000), (0, 1280), (230, 1280), (210, 1180), (190, 1090), (175, 1060)]
rp = [(W, 1000), (W, 1280), (520, 1280), (540, 1170), (560, 1080), (580, 1055)]
cd.polygon(lp, fill=hexc("#3b4a3a"))
cd.polygon(rp, fill=hexc("#33423a"))
for y in range(1050, H, 12):
    cd.line([(0, y), (200 - (y - 1050) * 0.05, y + 4)], fill=hexc("#2b3a2e"), width=2)
    cd.line([(W, y + 6), (560 + (y - 1050) * 0.05, y + 2)], fill=hexc("#28372b"), width=2)
bg = Image.alpha_composite(bg, cliff)
# Grass on the near banks, where the walk is.
grass = Image.new("RGBA", (W, H), (0, 0, 0, 0))
gr = ImageDraw.Draw(grass)
gr.polygon([(0, 1000), (180, 1060), (175, 1080), (0, 1060)], fill=hexc("#6f8f4e"))
gr.polygon([(W, 1000), (575, 1058), (580, 1080), (W, 1060)], fill=hexc("#5f7f46"))
for i in range(160):
    side = rnd.random() < 0.5
    x = rnd.randrange(0, 175) if side else rnd.randrange(580, W)
    y = 1000 + (x if side else W - x) * 0.33 + rnd.randrange(-2, 30)
    gr.line([(x, y), (x + rnd.randrange(-3, 4), y - rnd.randrange(6, 16))], fill=hexc("#8fb060") if rnd.random() < 0.5 else hexc("#4e6e3a"), width=2)
bg = Image.alpha_composite(bg, grass)
# The stone bridge across the gorge: deck at y 1040 (canvas 520), an arch under it.
br = Image.new("RGBA", (W, H), (0, 0, 0, 0))
bd = ImageDraw.Draw(br)
deck_y = 1040
bd.rectangle([0, deck_y, W, deck_y + 46], fill=hexc("#8a7d72"))
bd.rectangle([0, deck_y, W, deck_y + 8], fill=hexc("#b3a597"))
# The arch: the bridge's underside, cut by a half ellipse.
bd.rectangle([120, deck_y + 46, 600, deck_y + 130], fill=hexc("#7a6e64"))
bd.ellipse([150, deck_y + 70, 570, deck_y + 330], fill=(0, 0, 0, 0))
arch = Image.new("L", (W, H), 0)
ImageDraw.Draw(arch).ellipse([150, deck_y + 70, 570, deck_y + 330], fill=255)
br.putalpha(Image.composite(Image.new("L", (W, H), 0), br.getchannel("A"), arch))
bd = ImageDraw.Draw(br)
# Stones: rows of blocks.
for row in range(0, 6):
    y0 = deck_y + 8 + row * 20
    off = (row % 2) * 22
    for x in range(-off, W, 44):
        if br.getpixel((max(0, min(W - 1, x + 20)), min(H - 1, y0 + 10)))[3] == 0:
            continue
        bd.rectangle([x + 1, y0 + 1, x + 42, y0 + 18], outline=hexc("#5d524a"), width=2)
# The parapet: low posts and a rail.
for x in range(10, W, 70):
    bd.rectangle([x, deck_y - 34, x + 14, deck_y], fill=hexc("#9b8e82"), outline=hexc("#5d524a"), width=2)
bd.rectangle([0, deck_y - 38, W, deck_y - 30], fill=hexc("#a89a8c"), outline=hexc("#5d524a"), width=2)
# Moss on the stones, and a lantern on a post near the gate.
for i in range(90):
    x = rnd.randrange(0, W)
    y = deck_y + rnd.randrange(0, 120)
    if br.getpixel((x, y))[3]:
        bd.ellipse([x - 4, y - 2, x + 4, y + 2], fill=hexc("#5f7f46", 200))
bg = Image.alpha_composite(bg, br)
# Warm light from the lantern by the gate (canvas x 300).
lamp = Image.new("RGBA", (W, H), (0, 0, 0, 0))
ld = ImageDraw.Draw(lamp)
for r in range(170, 0, -5):
    ld.ellipse([600 - r, 900 - r, 600 + r, 900 + r], fill=(255, 196, 110, int(60 * (1 - r / 170) ** 2)))
bg = Image.alpha_composite(bg, lamp)
d = ImageDraw.Draw(bg)
d.rectangle([594, 905, 604, deck_y - 30], fill=hexc("#3b2a22"))
d.rectangle([586, 878, 612, 908], fill=hexc("#ffd27a"), outline=hexc("#3b2a22"), width=3)
d.line([(586, 893), (612, 893)], fill=hexc("#3b2a22"), width=2)
# A vignette, gently.
vig = Image.new("L", (W, H), 0)
vp = vig.load()
for y in range(H):
    for x in range(0, W):
        dx, dy = (x - W / 2) / (W * 0.75), (y - H * 0.55) / (H * 0.7)
        vp[x, y] = int(255 * max(0.0, min(1.0, 1.25 - (dx * dx + dy * dy))))
dark = Image.new("RGBA", (W, H), (20, 12, 30, 255))
bg = Image.composite(bg, dark, vig.point(lambda v: min(255, 120 + v)))
bg.convert("RGB").save(f"{out}/images/bridge.jpg", quality=90)

# ---------------------------------------------------------------------------
# Gruk on the bridge (pixel art, 2 sheet pixels an art pixel, drawn crisp).


def gruk_body():
    c = G
    s = Px(56, 72)
    # Legs and big feet.
    s.rect(16, 54, 24, 66, c["skin_lo"]); s.rect(32, 54, 40, 66, c["skin_lo"])
    s.ellipse(19, 67, 7, 4, c["skin_dk"]); s.ellipse(37, 67, 7, 4, c["skin_dk"])
    # Body: a barrel in a vest.
    s.ellipse(28, 44, 20, 17, c["skin"])
    s.ellipse(28, 46, 16, 14, c["vest"])
    s.rect(24, 32, 32, 60, c["shirt"])
    s.shade(lambda x, y: x > 36 and s.get(x, y) == c["vest"], hexc("#5f3c22"))
    s.rect(12, 55, 44, 59, c["strap"])
    # Arms: one on a club.
    s.ellipse(8, 44, 5, 12, c["skin"]); s.ellipse(48, 44, 5, 12, c["skin"])
    s.ellipse(6, 56, 5, 4, c["skin_lo"]); s.ellipse(50, 56, 5, 4, c["skin_lo"])
    s.poly([(50, 54), (55, 30), (53, 28), (47, 54)], hexc("#6b4a2f"))
    s.ellipse(54, 28, 3, 4, hexc("#7d5636"))
    # Head.
    s.ellipse(28, 17, 13, 12, c["skin"])
    s.ellipse(28, 24, 14, 8, c["skin"])
    s.shade(lambda x, y: (x - 23) ** 2 + (y - 11) ** 2 < 24, c["skin_hi"])
    s.poly([(15, 15), (10, 9), (12, 19)], c["skin_lo"]); s.poly([(41, 15), (46, 9), (44, 19)], c["skin_lo"])
    for x in range(20, 37, 2):
        s.rect(x, 4 + (x % 3), x + 2, 7, c["hair"])
    s.ellipse(23, 17, 2, 1.6, c["eye"]); s.ellipse(33, 17, 2, 1.6, c["eye"])
    s.set(23, 17, c["pupil"]); s.set(33, 17, c["pupil"])
    s.line([(20, 14), (25, 15)], c["brow"]); s.line([(31, 15), (36, 14)], c["brow"])
    s.ellipse(28, 21, 3, 2.4, c["nose"])
    s.line([(22, 26), (34, 26)], c["mouth"])
    s.rect(23, 23, 25, 27, c["tusk"]); s.rect(31, 23, 33, 27, c["tusk"])
    s.outline(INK)
    return s


gruk_body().scaled(2).save(f"{out}/images/gruk.png")
with open(f"{out}/images/gruk.toml", "w") as f:
    f.write("pixel = true\n")

# Pip, a walker: the body (first cell) and one foot (second), pixel art.


def pip_walker():
    c = P
    body = Px(22, 26)
    body.ellipse(11, 18, 8, 8, c["hood"])
    body.shade(lambda x, y: x > 14 and body.get(x, y) == c["hood"], c["hood_lo"])
    body.ellipse(11, 10, 8.5, 8.5, c["hood"])
    body.poly([(8, 3), (11, -1), (14, 3)], c["hood"])
    body.ellipse(11, 11, 6, 6, c["lining"])
    body.ellipse(11, 11.5, 5, 5, c["skin"])
    body.rect(6, 7, 17, 9, c["hair"])
    body.set(9, 11, c["pupil"]); body.set(13, 11, c["pupil"])
    body.set(9, 10, c["eye"]); body.set(13, 10, c["eye"])
    body.ellipse(11, 17, 6, 2, c["scarf"])
    body.rect(14, 17, 16, 21, c["scarf"])
    # A little satchel.
    body.rect(4, 17, 8, 21, hexc("#8a5a34"))
    body.outline(INK)
    foot = Px(8, 6)
    foot.ellipse(4, 3, 3, 2, hexc("#5a3a26"))
    foot.outline(INK)
    k = 3
    sheet_img = Image.new("RGBA", (22 * k * 2, 26 * k), (0, 0, 0, 0))
    sheet_img.paste(body.scaled(k), (0, 0))
    sheet_img.paste(foot.scaled(k), (22 * k + 7 * k, 20 * k))
    sheet_img.save(f"{out}/images/pip.png")
    # Where the body and the foot are (what mb build would measure from walker = true).
    bb = body.scaled(k).getbbox()
    fb = foot.scaled(k).getbbox()
    fx, fy = 22 * k + 7 * k, 20 * k
    with open(f"{out}/images/pip.toml", "w") as f:
        f.write(f"# A walker: the body (the first cell) and one foot (the second).\nwalker = true\npixel = true\nframe = [{22 * k}, {26 * k}]\n"
                f"body = [{bb[0]}, {bb[1]}, {bb[2] - bb[0]}, {bb[3] - bb[1]}]\nfoot = [{fx + fb[0]}, {fy + fb[1]}, {fb[2] - fb[0]}, {fb[3] - fb[1]}]\n")


pip_walker()

# The toll gate: a post and a striped bar, shut then open (2 frames).


def gate(open_):
    s = Px(48, 48)
    s.rect(2, 16, 8, 48, hexc("#5a3a26"))
    s.rect(1, 14, 10, 18, hexc("#7a5236"))
    if open_:
        # The bar swung up on its pivot, nearly upright.
        for i in range(36):
            y = 15 - i * 0.4
            x = 7 + i * 0.12
            s.rect(x, y - 1, x + 4, y + 1, hexc("#c8463c") if (i // 6) % 2 else hexc("#f2ead2"))
    else:
        for x in range(8, 46):
            s.rect(x, 17, x + 1, 21, hexc("#c8463c") if ((x - 8) // 6) % 2 else hexc("#f2ead2"))
        s.rect(43, 21, 46, 48, hexc("#5a3a26"))
    s.ellipse(5, 16, 3, 3, hexc("#c9a24a"))
    s.outline(INK)
    return s


gimg = Image.new("RGBA", (48 * 2 * 2, 48 * 2), (0, 0, 0, 0))
gimg.paste(gate(False).scaled(2), (0, 0))
gimg.paste(gate(True).scaled(2), (96, 0))
gimg.save(f"{out}/images/gate.png")
with open(f"{out}/images/gate.toml", "w") as f:
    f.write("pixel = true\nframe = [96, 96]\n\n[anims]\nshut = { frames = \"1\" }\nopen = { frames = \"2\" }\n")
print("drawn")
