#!/usr/bin/env python3
"""Writes assets/fox.glb: a small rigged, animated fox for mb3d 2 (SPEC
§5.4 "Animated models"), built and animated entirely in code, so the game
stays remixable and nothing is copied from anywhere.

  skeleton   18 joints: Root > Hips > Spine > Neck > Head (> Ear.L, Ear.R),
             Tail1 > Tail2 > Tail3, four legs of two bones,
             plus Socket.Tail and Socket.Back. Sockets are empty nodes for the
             game to attach things to (a lantern hangs from the tail).
  mesh       one low-poly skinned mesh (ellipsoids, cones and tubes), vertex
             colors, ~1.1 k triangles; 4 joint weights per vertex, blended
             smoothly across the knees, spine, neck and tail
  morph      one target, "Blink" (the eyes close), the first of a glTF mesh's
             targets mb3d blends
  clips      Idle  2.4 s  CUBICSPLINE  breathing, tail swish, head turn, a blink
                                       (the blink is a STEP channel on the weights)
             Walk  1.0 s  LINEAR       a four-beat walk
             Run   0.6 s  LINEAR       a bounding gallop, ears back
             Jump  0.9 s  LINEAR       crouch, launch, tuck, land (play once)

The fox faces +z and stands on y = 0, about 0.9 m tall at the ears.
Stdlib only:  python3 tools/make_fox.py assets/fox.glb
"""
import json, math, os, struct, sys

OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.join("assets", "fox.glb")
TAU = math.tau

# --- skeleton: (name, parent, rest translation relative to the parent)
JOINTS = [
    ("Root", None, (0.0, 0.0, 0.0)),
    ("Hips", "Root", (0.0, 0.52, -0.22)),
    ("Spine", "Hips", (0.0, 0.04, 0.42)),
    ("Neck", "Spine", (0.0, 0.12, 0.14)),
    ("Head", "Neck", (0.0, 0.2, 0.1)),
    ("Ear.L", "Head", (0.08, 0.12, -0.02)),
    ("Ear.R", "Head", (-0.08, 0.12, -0.02)),
    ("Tail1", "Hips", (0.0, 0.06, -0.2)),
    ("Tail2", "Tail1", (0.0, 0.07, -0.22)),
    ("Tail3", "Tail2", (0.0, 0.0, -0.22)),
    ("Leg.FL", "Spine", (0.11, -0.08, 0.04)),
    ("Shin.FL", "Leg.FL", (0.0, -0.24, 0.0)),
    ("Leg.FR", "Spine", (-0.11, -0.08, 0.04)),
    ("Shin.FR", "Leg.FR", (0.0, -0.24, 0.0)),
    ("Leg.BL", "Hips", (0.11, -0.06, -0.02)),
    ("Shin.BL", "Leg.BL", (0.0, -0.24, 0.0)),
    ("Leg.BR", "Hips", (-0.11, -0.06, -0.02)),
    ("Shin.BR", "Leg.BR", (0.0, -0.24, 0.0)),
]
SOCKETS = [("Socket.Tail", "Tail3", (0.0, 0.02, -0.16)), ("Socket.Back", "Spine", (0.0, 0.17, -0.1))]
NAMES = [j[0] for j in JOINTS]
J = {n: i for i, n in enumerate(NAMES)}


def world_pos(name):
    """Rest-pose world position of a joint (rotations are identity at rest)."""
    for n, parent, t in JOINTS:
        if n == name:
            if parent is None:
                return t
            p = world_pos(parent)
            return (p[0] + t[0], p[1] + t[1], p[2] + t[2])
    raise KeyError(name)


# --- geometry: positions, normals, colors (linear rgba), joints, weights, blink deltas
P, N, C, JS, WS, BLINK, IDX = [], [], [], [], [], [], []

ORANGE = (0.82, 0.30, 0.07, 1.0)
CREAM = (0.92, 0.86, 0.76, 1.0)
DARK = (0.05, 0.03, 0.025, 1.0)
EYE = (0.01, 0.01, 0.015, 1.0)


def vertex(p, n, color, weights, blink=(0.0, 0.0, 0.0)):
    """weights: {joint name: w}; up to four kept, normalized."""
    ws = sorted(weights.items(), key=lambda kv: -kv[1])[:4]
    total = sum(w for _, w in ws) or 1.0
    js = [J[k] for k, _ in ws] + [0] * (4 - len(ws))
    wv = [w / total for _, w in ws] + [0.0] * (4 - len(ws))
    l = math.sqrt(sum(x * x for x in n)) or 1.0
    P.append(p)
    N.append(tuple(x / l for x in n))
    C.append(color)
    JS.append(js)
    WS.append(wv)
    BLINK.append(blink)
    return len(P) - 1


def smooth(e0, e1, x):
    t = max(0.0, min(1.0, (x - e0) / (e1 - e0)))
    return t * t * (3 - 2 * t)


def ellipsoid(c, r, segs, rings, color_of, weights_of, blink_of=None):
    base = len(P)
    for i in range(rings + 1):
        th = math.pi * i / rings
        for k in range(segs + 1):
            ph = TAU * k / segs
            d = (math.sin(th) * math.sin(ph), math.cos(th), math.sin(th) * math.cos(ph))
            p = (c[0] + r[0] * d[0], c[1] + r[1] * d[1], c[2] + r[2] * d[2])
            n = (d[0] / r[0], d[1] / r[1], d[2] / r[2])
            vertex(p, n, color_of(p, d), weights_of(p), blink_of(p, d) if blink_of else (0.0, 0.0, 0.0))
    row = segs + 1
    for i in range(rings):
        for k in range(segs):
            a, b = base + i * row + k, base + (i + 1) * row + k
            IDX.extend([a, b, a + 1, a + 1, b, b + 1])


def tube(points, radii, segs, color_of, weights_of):
    """A tube along a polyline, capped at both ends, with a radius per point."""
    base = len(P)
    frames = []
    for i, p in enumerate(points):
        a = points[max(i - 1, 0)]
        b = points[min(i + 1, len(points) - 1)]
        t = [b[k] - a[k] for k in range(3)]
        l = math.sqrt(sum(x * x for x in t)) or 1.0
        t = [x / l for x in t]
        helper = (1.0, 0.0, 0.0) if abs(t[0]) < 0.9 else (0.0, 1.0, 0.0)
        u = [helper[1] * t[2] - helper[2] * t[1], helper[2] * t[0] - helper[0] * t[2], helper[0] * t[1] - helper[1] * t[0]]
        lu = math.sqrt(sum(x * x for x in u))
        u = [x / lu for x in u]
        v = [t[1] * u[2] - t[2] * u[1], t[2] * u[0] - t[0] * u[2], t[0] * u[1] - t[1] * u[0]]
        frames.append((u, v, t))
    for i, p in enumerate(points):
        u, v, _ = frames[i]
        for k in range(segs + 1):
            a = TAU * k / segs
            d = [math.cos(a) * u[j] + math.sin(a) * v[j] for j in range(3)]
            q = tuple(p[j] + radii[i] * d[j] for j in range(3))
            vertex(q, d, color_of(q, i / (len(points) - 1)), weights_of(q, i / (len(points) - 1)))
    row = segs + 1
    for i in range(len(points) - 1):
        for k in range(segs):
            a, b = base + i * row + k, base + (i + 1) * row + k
            # Counter-clockwise seen from outside: the ring order runs around the tangent.
            IDX.extend([a, a + 1, b, a + 1, b + 1, b])
    for end, sign in ((0, -1.0), (len(points) - 1, 1.0)):
        p, (u, v, t) = points[end], frames[end]
        n = [x * sign for x in t]
        center = vertex(p, n, color_of(p, end / (len(points) - 1)), weights_of(p, end / (len(points) - 1)))
        ring = []
        for k in range(segs):
            a = TAU * k / segs
            d = [math.cos(a) * u[j] + math.sin(a) * v[j] for j in range(3)]
            q = tuple(p[j] + radii[end] * d[j] for j in range(3))
            ring.append(vertex(q, n, color_of(q, end / (len(points) - 1)), weights_of(q, end / (len(points) - 1))))
        for k in range(segs):
            a, b = ring[k], ring[(k + 1) % segs]
            IDX.extend([center, a, b] if sign > 0 else [center, b, a])


def cone(base_c, tip, r, segs, color, weights_of):
    """A closed cone from a circular base to a tip."""
    t = [tip[k] - base_c[k] for k in range(3)]
    l = math.sqrt(sum(x * x for x in t))
    t = [x / l for x in t]
    helper = (1.0, 0.0, 0.0) if abs(t[0]) < 0.9 else (0.0, 1.0, 0.0)
    u = [helper[1] * t[2] - helper[2] * t[1], helper[2] * t[0] - helper[0] * t[2], helper[0] * t[1] - helper[1] * t[0]]
    lu = math.sqrt(sum(x * x for x in u))
    u = [x / lu for x in u]
    v = [t[1] * u[2] - t[2] * u[1], t[2] * u[0] - t[0] * u[2], t[0] * u[1] - t[1] * u[0]]
    for k in range(segs):
        a0, a1 = TAU * k / segs, TAU * (k + 1) / segs
        d0 = [math.cos(a0) * u[j] + math.sin(a0) * v[j] for j in range(3)]
        d1 = [math.cos(a1) * u[j] + math.sin(a1) * v[j] for j in range(3)]
        p0 = tuple(base_c[j] + r * d0[j] for j in range(3))
        p1 = tuple(base_c[j] + r * d1[j] for j in range(3))
        # Side: flat-shaded facet.
        e1 = [p1[j] - p0[j] for j in range(3)]
        e2 = [tip[j] - p0[j] for j in range(3)]
        n = (e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0])
        tc = color(tip, 1.0)
        IDX.extend([vertex(p0, n, color(p0, 0.0), weights_of(p0)), vertex(p1, n, color(p1, 0.0), weights_of(p1)), vertex(tip, n, tc, weights_of(tip))])
        nb = [-x for x in t]
        IDX.extend([vertex(base_c, nb, color(base_c, 0.0), weights_of(base_c)), vertex(p1, nb, color(p1, 0.0), weights_of(p1)), vertex(p0, nb, color(p0, 0.0), weights_of(p0))])


def build():
    hips, spine, neck, head = (world_pos(n) for n in ("Hips", "Spine", "Neck", "Head"))
    # Body: hips → chest, cream belly.
    body_c = (0.0, 0.56, 0.0)

    def body_w(p):
        t = smooth(-0.25, 0.22, p[2])
        return {"Hips": 1 - t, "Spine": t}

    ellipsoid(body_c, (0.17, 0.16, 0.36), 14, 10, lambda p, d: CREAM if d[1] < -0.45 and abs(d[0]) < 0.6 else ORANGE, body_w)
    # Neck.
    tube([(0.0, 0.6, 0.22), (0.0, 0.7, 0.3), (0.0, 0.8, 0.34)], [0.11, 0.09, 0.085], 10, lambda p, s: CREAM if p[2] > 0.3 and p[1] < 0.75 else ORANGE,
         lambda p, s: {"Spine": max(0.0, 1 - 2 * s), "Neck": 1 - abs(s - 0.5) * 2, "Head": max(0.0, 2 * s - 1)})
    # Head, snout, nose, eyes, ears.
    hc = (0.0, head[1] + 0.02, head[2] + 0.02)
    ellipsoid(hc, (0.13, 0.11, 0.13), 12, 8, lambda p, d: CREAM if d[1] < -0.2 and d[2] > 0.1 else ORANGE, lambda p: {"Head": 1.0})
    cone((0.0, hc[1] - 0.02, hc[2] + 0.08), (0.0, hc[1] - 0.04, hc[2] + 0.27), 0.065, 8, lambda p, s: CREAM, lambda p: {"Head": 1.0})
    ellipsoid((0.0, hc[1] - 0.035, hc[2] + 0.27), (0.022, 0.018, 0.02), 6, 4, lambda p, d: DARK, lambda p: {"Head": 1.0})
    for side in (1, -1):
        ec = (0.06 * side, hc[1] + 0.025, hc[2] + 0.105)
        # Blink: every eye vertex moves to the eye's horizontal midline (flattened).
        ellipsoid(ec, (0.022, 0.026, 0.012), 6, 4, lambda p, d: EYE, lambda p: {"Head": 1.0},
                  lambda p, d, ec=ec: (0.0, (ec[1] - p[1]) * 0.92, 0.0))
        ear = world_pos("Ear.L" if side > 0 else "Ear.R")
        cone((ear[0], ear[1] - 0.01, ear[2]), (ear[0] + 0.03 * side, ear[1] + 0.15, ear[2] - 0.02), 0.05, 6,
             lambda p, s, ear=ear: DARK if p[1] > ear[1] + 0.09 else ORANGE, lambda p, side=side: {"Ear.L" if side > 0 else "Ear.R": 1.0})
    # Tail: bushy, cream tip, blended along its three bones.
    t1, t2, t3 = (world_pos(n) for n in ("Tail1", "Tail2", "Tail3"))
    tip = world_pos("Tail3")
    pts = [(t1[0], t1[1], t1[2] + 0.04), t1, t2, ((t2[0] + t3[0]) / 2, (t2[1] + t3[1]) / 2, (t2[2] + t3[2]) / 2), t3, (tip[0], tip[1] - 0.02, tip[2] - 0.16)]

    def tail_w(p, s):
        x = s * 3.0
        return {"Tail1": max(0.0, 1 - x), "Tail2": max(0.0, 1 - abs(x - 1.2)), "Tail3": max(0.0, min(1.0, x - 1.4))} if x < 3 else {"Tail3": 1.0}

    tube(pts, [0.05, 0.08, 0.11, 0.115, 0.09, 0.02], 10, lambda p, s: CREAM if s > 0.72 else ORANGE, tail_w)
    # Legs: upper and shin bones, dark socks.
    for leg in ("FL", "FR", "BL", "BR"):
        top, knee = world_pos("Leg." + leg), world_pos("Shin." + leg)
        foot = (knee[0], 0.02, knee[2] + 0.03)
        upper, shin = "Leg." + leg, "Shin." + leg

        def leg_w(p, s, upper=upper, shin=shin, knee=knee):
            t = smooth(knee[1] + 0.06, knee[1] - 0.06, p[1])
            return {upper: 1 - t, shin: t}

        tube([(top[0], top[1] + 0.06, top[2]), top, knee, foot], [0.075, 0.06, 0.042, 0.038], 8, lambda p, s: DARK if p[1] < 0.16 else ORANGE, leg_w)


# --- animation helpers

def qaxis(axis, angle):
    s = math.sin(angle / 2)
    return (axis[0] * s, axis[1] * s, axis[2] * s, math.cos(angle / 2))


def qmul(a, b):
    ax, ay, az, aw = a
    bx, by, bz, bw = b
    return (aw * bx + ax * bw + ay * bz - az * by, aw * by - ax * bz + ay * bw + az * bx, aw * bz + ax * by - ay * bx + az * bw, aw * bw - ax * bx - ay * by - az * bz)


def euler(x=0.0, y=0.0, z=0.0):
    return qmul(qmul(qaxis((0, 1, 0), y), qaxis((1, 0, 0), x)), qaxis((0, 0, 1), z))


def rest(name):
    for n, _, t in JOINTS:
        if n == name:
            return t
    raise KeyError(name)


def clip(name, duration, keys, channels, cubic=False):
    """channels: [(joint, path, f(t) -> tuple)]; sampled at `keys` + 1 evenly spaced times (loops close)."""
    times = [duration * i / keys for i in range(keys + 1)]
    out = []
    for joint, path, fn in channels:
        vals = [fn(t) for t in times]
        if cubic:
            # Hermite tangents from central differences of the sampled function (per second).
            h = duration / keys / 2
            data = []
            for t, v in zip(times, vals):
                a, b = fn(t - h), fn(t + h)
                tan = tuple((b[k] - a[k]) / (2 * h) for k in range(len(v)))
                data.append((tan, v, tan))
            out.append((joint, path, times, data, "CUBICSPLINE"))
        else:
            out.append((joint, path, times, vals, "LINEAR"))
    return name, out


def translate(name, off):
    r = rest(name)
    return (r[0] + off[0], r[1] + off[1], r[2] + off[2])


def clips():
    s = math.sin
    idle = clip("Idle", 2.4, 24, [
        ("Hips", "translation", lambda t: translate("Hips", (0.0, 0.008 * s(TAU * t / 2.4), 0.0))),
        ("Spine", "rotation", lambda t: euler(x=-0.03 * s(TAU * t / 2.4))),
        ("Neck", "rotation", lambda t: euler(x=0.05 + 0.04 * s(TAU * t / 2.4 + 0.6))),
        ("Head", "rotation", lambda t: euler(y=0.25 * s(TAU * t / 2.4), z=0.06 * s(TAU * t / 1.2))),
        ("Tail1", "rotation", lambda t: euler(x=0.35, y=0.45 * s(TAU * t / 1.2))),
        ("Tail2", "rotation", lambda t: euler(x=-0.15, y=0.35 * s(TAU * t / 1.2 - 0.8))),
        ("Tail3", "rotation", lambda t: euler(x=-0.2, y=0.3 * s(TAU * t / 1.2 - 1.6))),
        ("Ear.L", "rotation", lambda t: euler(z=-0.1 * max(0.0, s(TAU * t / 2.4 * 2)) ** 8)),
    ], cubic=True)

    def gait(period, swing, lift, phases, bob, spine_flex, ears, tail_lift):
        ch = [
            ("Hips", "translation", lambda t: translate("Hips", (0.0, bob * abs(s(TAU * t / period * 2)), 0.0))),
            ("Spine", "rotation", lambda t: euler(x=spine_flex * s(TAU * t / period))),
            ("Neck", "rotation", lambda t: euler(x=0.08 - spine_flex * 0.7 * s(TAU * t / period))),
            ("Tail1", "rotation", lambda t: euler(x=tail_lift + 0.12 * s(TAU * t / period), y=0.15 * s(TAU * t / period))),
            ("Tail2", "rotation", lambda t: euler(x=-0.1 + 0.15 * s(TAU * t / period - 0.7))),
            ("Ear.L", "rotation", lambda t: euler(x=-ears, z=-0.15 * ears)),
            ("Ear.R", "rotation", lambda t: euler(x=-ears, z=0.15 * ears)),
        ]
        for leg, ph in phases.items():
            ch.append(("Leg." + leg, "rotation", lambda t, ph=ph: euler(x=-swing * s(TAU * (t / period + ph)))))
            ch.append(("Shin." + leg, "rotation", lambda t, ph=ph: euler(x=lift * max(0.0, s(TAU * (t / period + ph) - 1.2)))))
        return ch

    walk = clip("Walk", 1.0, 20, gait(1.0, 0.45, 0.7, {"FL": 0.0, "BR": 0.25, "FR": 0.5, "BL": 0.75}, 0.015, 0.04, 0.0, 0.3))
    run = clip("Run", 0.6, 18, gait(0.6, 0.85, 1.1, {"FL": 0.0, "FR": 0.08, "BL": 0.5, "BR": 0.58}, 0.06, 0.16, 0.6, 0.05))

    def jump_h(t):
        # Crouch (0–0.15), airborne (0.15–0.75), land (0.75–0.9).
        if t < 0.15:
            return -0.1 * smooth(0.0, 0.15, t)
        if t < 0.75:
            u = (t - 0.15) / 0.6
            return -0.1 + 0.1 * smooth(0.0, 0.1, u) + 0.45 * 4 * u * (1 - u)
        return -0.08 * s(math.pi * (t - 0.75) / 0.15)

    def tuck(t):
        return smooth(0.18, 0.3, t) * (1 - smooth(0.55, 0.75, t))

    jump = clip("Jump", 0.9, 27, [
        ("Hips", "translation", lambda t: translate("Hips", (0.0, jump_h(t), 0.0))),
        ("Hips", "rotation", lambda t: euler(x=-0.35 * smooth(0.1, 0.25, t) * (1 - smooth(0.3, 0.55, t)) + 0.3 * smooth(0.45, 0.7, t) * (1 - smooth(0.75, 0.9, t)))),
        ("Spine", "rotation", lambda t: euler(x=0.15 * tuck(t))),
        ("Leg.FL", "rotation", lambda t: euler(x=-1.0 * tuck(t))),
        ("Leg.FR", "rotation", lambda t: euler(x=-1.0 * tuck(t))),
        ("Shin.FL", "rotation", lambda t: euler(x=1.4 * tuck(t))),
        ("Shin.FR", "rotation", lambda t: euler(x=1.4 * tuck(t))),
        ("Leg.BL", "rotation", lambda t: euler(x=0.9 * tuck(t))),
        ("Leg.BR", "rotation", lambda t: euler(x=0.9 * tuck(t))),
        ("Shin.BL", "rotation", lambda t: euler(x=-0.6 * tuck(t))),
        ("Shin.BR", "rotation", lambda t: euler(x=-0.6 * tuck(t))),
        ("Tail1", "rotation", lambda t: euler(x=0.2 + 0.5 * tuck(t))),
        ("Ear.L", "rotation", lambda t: euler(x=-0.7 * tuck(t))),
        ("Ear.R", "rotation", lambda t: euler(x=-0.7 * tuck(t))),
    ])
    return [idle, walk, run, jump]


# --- glTF binary writer

class Bin:
    def __init__(self):
        self.data = bytearray()
        self.views, self.accessors = [], []

    def add(self, raw, target=None):
        while len(self.data) % 4:
            self.data.append(0)
        view = {"buffer": 0, "byteOffset": len(self.data), "byteLength": len(raw)}
        if target:
            view["target"] = target
        self.data.extend(raw)
        self.views.append(view)
        return len(self.views) - 1

    def accessor(self, fmt, values, ctype, typ, count, target=None, normalized=False, minmax=False):
        raw = b"".join(struct.pack("<" + fmt, *v) if isinstance(v, (tuple, list)) else struct.pack("<" + fmt, v) for v in values)
        a = {"bufferView": self.add(raw, target), "componentType": ctype, "count": count, "type": typ}
        if normalized:
            a["normalized"] = True
        if minmax:
            n = len(values[0])
            a["min"] = [min(v[k] for v in values) for k in range(n)]
            a["max"] = [max(v[k] for v in values) for k in range(n)]
        self.accessors.append(a)
        return len(self.accessors) - 1


def glb():
    build()
    b = Bin()
    nv = len(P)
    pos = b.accessor("3f", P, 5126, "VEC3", nv, 34962, minmax=True)
    nrm = b.accessor("3f", N, 5126, "VEC3", nv, 34962)
    col = b.accessor("4B", [tuple(int(round(max(0.0, min(1.0, x)) * 255)) for x in c) for c in C], 5121, "VEC4", nv, 34962, normalized=True)
    jnt = b.accessor("4B", JS, 5121, "VEC4", nv, 34962)
    wgt = b.accessor("4f", WS, 5126, "VEC4", nv, 34962)
    blink = b.accessor("3f", BLINK, 5126, "VEC3", nv, 34962, minmax=True)
    idx = b.accessor("H", IDX, 5123, "SCALAR", len(IDX), 34963)
    # Inverse bind matrices: joints are only translated at rest, so each is a translation by −(world position).
    ibm = []
    for n in NAMES:
        w = world_pos(n)
        ibm.append((1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, -w[0], -w[1], -w[2], 1))
    ibm_acc = b.accessor("16f", ibm, 5126, "MAT4", len(ibm))

    # Nodes: 0 = the scene root "Fox" (holds the mesh), then joints, then sockets.
    nodes = [{"name": "Fox", "children": [1, len(JOINTS) + 1], "mesh": 0, "skin": 0}]
    for i, (n, parent, t) in enumerate(JOINTS):
        nodes.append({"name": n, "translation": list(t), "children": []})
    for i, (n, parent, t) in enumerate(JOINTS):
        if parent:
            nodes[J[parent] + 1]["children"].append(i + 1)
    for n, parent, t in SOCKETS:
        nodes.append({"name": n, "translation": list(t)})
        nodes[J[parent] + 1]["children"].append(len(nodes) - 1)
    # The mesh node sits beside the skeleton (skinned meshes ignore their own transform).
    nodes[0]["children"] = [1]
    for nd in nodes:
        if "children" in nd and not nd["children"]:
            del nd["children"]

    animations = []
    for name, chans in clips():
        samplers, channels = [], []
        for joint, path, times, data, interp in chans:
            tin = b.accessor("f", times, 5126, "SCALAR", len(times), minmax=False)
            b.accessors[tin]["min"] = [times[0]]
            b.accessors[tin]["max"] = [times[-1]]
            flat = []
            for d in data:
                if interp == "CUBICSPLINE":
                    for part in d:
                        flat.append(part)
                else:
                    flat.append(d)
            n = len(flat[0])
            out = b.accessor(f"{n}f", flat, 5126, "VEC3" if n == 3 else "VEC4", len(flat))
            samplers.append({"input": tin, "output": out, "interpolation": interp})
            channels.append({"sampler": len(samplers) - 1, "target": {"node": J[joint] + 1, "path": path}})
        if name == "Idle":
            # A blink at 1.6 s (STEP: eyes shut for ~0.12 s).
            times = [0.0, 1.6, 1.72, 2.4]
            tin = b.accessor("f", times, 5126, "SCALAR", 4)
            b.accessors[tin]["min"], b.accessors[tin]["max"] = [0.0], [2.4]
            out = b.accessor("f", [0.0, 1.0, 0.0, 0.0], 5126, "SCALAR", 4)
            samplers.append({"input": tin, "output": out, "interpolation": "STEP"})
            channels.append({"sampler": len(samplers) - 1, "target": {"node": 0, "path": "weights"}})
        animations.append({"name": name, "samplers": samplers, "channels": channels})

    doc = {
        "asset": {"version": "2.0", "generator": "mistwood tools/make_fox.py"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": nodes,
        "skins": [{"joints": list(range(1, len(JOINTS) + 1)), "inverseBindMatrices": ibm_acc, "skeleton": 1}],
        "meshes": [{
            "name": "Fox",
            "primitives": [{
                "attributes": {"POSITION": pos, "NORMAL": nrm, "COLOR_0": col, "JOINTS_0": jnt, "WEIGHTS_0": wgt},
                "indices": idx,
                "material": 0,
                "targets": [{"POSITION": blink}],
            }],
            "weights": [0.0],
            "extras": {"targetNames": ["Blink"]},
        }],
        "materials": [{"name": "Fur", "pbrMetallicRoughness": {"baseColorFactor": [1, 1, 1, 1], "metallicFactor": 0.0, "roughnessFactor": 0.75}}],
        "animations": animations,
        "buffers": [{"byteLength": 0}],
        "bufferViews": b.views,
        "accessors": b.accessors,
    }
    while len(b.data) % 4:
        b.data.append(0)
    doc["buffers"][0]["byteLength"] = len(b.data)
    j = json.dumps(doc, separators=(",", ":")).encode()
    while len(j) % 4:
        j += b" "
    total = 12 + 8 + len(j) + 8 + len(b.data)
    out = struct.pack("<4sII", b"glTF", 2, total) + struct.pack("<I4s", len(j), b"JSON") + j + struct.pack("<I4s", len(b.data), b"BIN\x00") + bytes(b.data)
    os.makedirs(os.path.dirname(OUT) or ".", exist_ok=True)
    with open(OUT, "wb") as f:
        f.write(out)
    print(f"wrote {OUT}: {nv} vertices, {len(IDX) // 3} triangles, {len(JOINTS)} joints, {len(animations)} clips, {len(out)} bytes")


if __name__ == "__main__":
    glb()
