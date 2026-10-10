#!/usr/bin/env python3
"""Generates assets/relic.glb: a small textured glTF model for the mb3d demo.

A cube "relic" whose detail is all in its textures, generated here from one
height field: engraved panels, a beveled rim, rivets and an embossed
emblem, with glowing runes in the grooves.

  base color   JPEG (sRGB)   bronze panels, dark grooves, enamel emblem
  normal map   PNG (linear)  from the height field's slopes
  metal-rough  PNG (linear)  glTF layout: roughness in G, metallic in B
  emissive     PNG (sRGB)    the runes

Stdlib only, plus two optional tools: `sips` (macOS) to write the JPEG
(otherwise the base color stays PNG), and `gltfpack` (npm) to quantize the
mesh and compress it with meshopt (EXT_meshopt_compression), which is what
mb3d's loader is built to read. Without gltfpack the plain .glb is kept.

    python3 tools/make_relic.py            # from games/mb3d-demo
"""
import json, math, os, shutil, struct, subprocess, sys, tempfile, zlib

N = 256  # texture size
HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "assets", "relic.glb")


def png(w, h, rows, channels=4):
    """rows: list of bytes, w*channels each."""
    raw = b"".join(b"\x00" + r for r in rows)
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    color = {3: 2, 4: 6}[channels]
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, color, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


def clamp(v, lo=0.0, hi=1.0):
    return lo if v < lo else hi if v > hi else v


def smooth(e0, e1, x):
    t = clamp((x - e0) / (e1 - e0))
    return t * t * (3 - 2 * t)


# --- the height field and material masks, per texel (u, v in 0..1)

def fields(u, v):
    """Returns (height, groove, emblem, rune) for one texel."""
    x, y = u * 2 - 1, v * 2 - 1
    edge = max(abs(x), abs(y))
    h = 1.0 - smooth(0.82, 0.98, edge) * 0.6          # beveled rim
    # Four panels separated by a cross-shaped groove and an inner frame.
    groove = max(1 - smooth(0.0, 0.035, abs(x)), 1 - smooth(0.0, 0.035, abs(y)))
    frame = 1 - smooth(0.0, 0.03, abs(edge - 0.74))
    groove = max(groove, frame)
    h -= 0.35 * groove
    # Rivets at the panel corners.
    for cx in (-0.62, -0.12, 0.12, 0.62):
        for cy in (-0.62, -0.12, 0.12, 0.62):
            d = math.hypot(x - cx, y - cy)
            h += 0.25 * (1 - smooth(0.02, 0.055, d))
    # An embossed emblem: a ring with a diamond in the middle.
    r = math.hypot(x, y)
    ring = (1 - smooth(0.0, 0.03, abs(r - 0.36)))
    diamond = 1 - smooth(0.17, 0.2, abs(x) + abs(y))
    emblem = max(ring, diamond)
    h += 0.3 * emblem
    # Runes: short dashes along the groove, lit from inside.
    t = (y if abs(x) < 0.04 else x) * 9.0
    dash = 1 - smooth(0.25, 0.4, abs(t - round(t)))
    rune = groove * dash * (1 - smooth(0.66, 0.72, edge))
    return h, groove, emblem, rune


def build_textures():
    H = [[0.0] * N for _ in range(N)]
    M = [[None] * N for _ in range(N)]
    for j in range(N):
        for i in range(N):
            h, g, e, rn = fields((i + 0.5) / N, (j + 0.5) / N)
            H[j][i] = h
            M[j][i] = (g, e, rn)
    base, normal, mr, emis = [], [], [], []
    for j in range(N):
        b, n, m, em = bytearray(), bytearray(), bytearray(), bytearray()
        for i in range(N):
            g, e, rn = M[j][i]
            # Slopes → tangent-space normal (+y up in the image is −v).
            dx = (H[j][min(i + 1, N - 1)] - H[j][max(i - 1, 0)]) * N * 0.012
            dy = (H[min(j + 1, N - 1)][i] - H[max(j - 1, 0)][i]) * N * 0.012
            nx, ny, nz = -dx, dy, 1.0
            l = math.sqrt(nx * nx + ny * ny + nz * nz)
            n += bytes(int((c / l * 0.5 + 0.5) * 255 + 0.5) for c in (nx, ny, nz))
            # Bronze panels with a little grain; dark grooves; teal enamel emblem.
            grain = 0.92 + 0.08 * math.sin(i * 0.9 + math.sin(j * 0.21) * 3.0)
            bronze = (0.80 * grain, 0.55 * grain, 0.32 * grain)
            enamel = (0.08, 0.42, 0.45)
            dark = (0.05, 0.04, 0.035)
            c = [bronze[k] * (1 - e) + enamel[k] * e for k in range(3)]
            c = [c[k] * (1 - g) + dark[k] * g for k in range(3)]
            b += bytes(int(clamp(x) * 255 + 0.5) for x in c)
            # Roughness (G) and metallic (B): polished metal, rough grooves, glossy enamel.
            rough = 0.42 + 0.4 * g - 0.2 * e
            metal = (1 - g) * (1 - e)
            m += bytes((255, int(clamp(rough) * 255), int(clamp(metal) * 255)))
            em += bytes(int(clamp(rn * k) * 255) for k in (0.3, 0.9, 1.0))
        base.append(bytes(b))
        normal.append(bytes(n))
        mr.append(bytes(m))
        emis.append(bytes(em))
    return png(N, N, base, 3), png(N, N, normal, 3), png(N, N, mr, 3), png(N, N, emis, 3)


def to_jpeg(png_bytes):
    """PNG → JPEG with macOS sips; None if unavailable."""
    if not shutil.which("sips"):
        return None
    with tempfile.TemporaryDirectory() as d:
        src, dst = os.path.join(d, "in.png"), os.path.join(d, "out.jpg")
        open(src, "wb").write(png_bytes)
        r = subprocess.run(["sips", "-s", "format", "jpeg", "-s", "formatOptions", "88", src, "--out", dst], capture_output=True)
        return open(dst, "rb").read() if r.returncode == 0 else None


def cube():
    """24 vertices (4 per face, so each face maps the whole texture) and 36 indices."""
    pos, nrm, uv, idx = [], [], [], []
    faces = [((1, 0, 0), (0, 0, -1), (0, 1, 0)), ((-1, 0, 0), (0, 0, 1), (0, 1, 0)), ((0, 1, 0), (1, 0, 0), (0, 0, -1)),
             ((0, -1, 0), (1, 0, 0), (0, 0, 1)), ((0, 0, 1), (1, 0, 0), (0, 1, 0)), ((0, 0, -1), (-1, 0, 0), (0, 1, 0))]
    for n, u, v in faces:
        base = len(pos)
        for su, sv, tu, tv in ((-1, -1, 0, 1), (1, -1, 1, 1), (1, 1, 1, 0), (-1, 1, 0, 0)):
            pos.append(tuple(n[k] + u[k] * su + v[k] * sv for k in range(3)))
            nrm.append(n)
            uv.append((tu, tv))
        idx += [base, base + 1, base + 2, base, base + 2, base + 3]
    return pos, nrm, uv, idx


def glb(textures, base_is_jpeg):
    pos, nrm, uv, idx = cube()
    bin_ = bytearray()
    views, accessors = [], []

    def add_view(data, target=None, stride=None):
        while len(bin_) % 4:
            bin_.append(0)
        v = {"buffer": 0, "byteOffset": len(bin_), "byteLength": len(data)}
        if target:
            v["target"] = target
        if stride:
            v["byteStride"] = stride
        bin_.extend(data)
        views.append(v)
        return len(views) - 1

    def add_accessor(values, comps, typ, extra=None):
        data = b"".join(struct.pack("<" + "f" * comps, *v) for v in values)
        a = {"bufferView": add_view(data, 34962), "componentType": 5126, "count": len(values), "type": typ}
        a.update(extra or {})
        accessors.append(a)
        return len(accessors) - 1

    lo = [min(p[k] for p in pos) for k in range(3)]
    hi = [max(p[k] for p in pos) for k in range(3)]
    p_acc = add_accessor(pos, 3, "VEC3", {"min": lo, "max": hi})
    n_acc = add_accessor(nrm, 3, "VEC3")
    t_acc = add_accessor(uv, 2, "VEC2")
    accessors.append({"bufferView": add_view(struct.pack("<%dH" % len(idx), *idx), 34963), "componentType": 5123, "count": len(idx), "type": "SCALAR"})
    i_acc = len(accessors) - 1

    images = []
    for data, mime in textures:
        images.append({"bufferView": add_view(data), "mimeType": mime})
    doc = {
        "asset": {"version": "2.0", "generator": "games/mb3d-demo/tools/make_relic.py"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"name": "relic", "mesh": 0}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": p_acc, "NORMAL": n_acc, "TEXCOORD_0": t_acc}, "indices": i_acc, "material": 0}]}],
        "materials": [{
            "name": "bronze",
            "pbrMetallicRoughness": {"baseColorTexture": {"index": 0}, "metallicRoughnessTexture": {"index": 2}, "metallicFactor": 1.0, "roughnessFactor": 1.0},
            "normalTexture": {"index": 1},
            "emissiveTexture": {"index": 3},
            "emissiveFactor": [1.0, 1.0, 1.0],
            "extensions": {"KHR_materials_emissive_strength": {"emissiveStrength": 6.0}},
        }],
        "extensionsUsed": ["KHR_materials_emissive_strength"],
        "textures": [{"source": i} for i in range(4)],
        "images": images,
        "buffers": [{"byteLength": len(bin_)}],
        "bufferViews": views,
        "accessors": accessors,
    }
    js = json.dumps(doc, separators=(",", ":")).encode()
    js += b" " * (-len(js) % 4)
    while len(bin_) % 4:
        bin_.append(0)
    total = 12 + 8 + len(js) + 8 + len(bin_)
    return b"glTF" + struct.pack("<II", 2, total) + struct.pack("<I", len(js)) + b"JSON" + js + struct.pack("<I", len(bin_)) + b"BIN\x00" + bytes(bin_)


def gltfpack():
    """A gltfpack command: on PATH, or through npx."""
    if shutil.which("gltfpack"):
        return ["gltfpack"]
    if shutil.which("npx"):
        return ["npx", "--yes", "gltfpack@1.3.0"]
    return None


def main():
    base, normal, mr, emis = build_textures()
    jpg = to_jpeg(base)
    textures = [(jpg or base, "image/jpeg" if jpg else "image/png"), (normal, "image/png"), (mr, "image/png"), (emis, "image/png")]
    raw = glb(textures, jpg is not None)
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    cmd = gltfpack()
    if cmd:
        with tempfile.TemporaryDirectory() as d:
            src = os.path.join(d, "relic_raw.glb")
            open(src, "wb").write(raw)
            # -c: meshopt compression; quantized attributes by default; textures are kept as they are.
            r = subprocess.run(cmd + ["-i", src, "-o", OUT, "-c", "-kn", "-km"], capture_output=True, text=True)
            if r.returncode == 0:
                print(f"wrote {OUT}: {os.path.getsize(OUT)} bytes (meshopt, quantized; raw was {len(raw)})")
                return
            print("gltfpack failed, writing the plain .glb:", r.stderr.strip(), file=sys.stderr)
    open(OUT, "wb").write(raw)
    print(f"wrote {OUT}: {len(raw)} bytes (no gltfpack: uncompressed)")


if __name__ == "__main__":
    main()
