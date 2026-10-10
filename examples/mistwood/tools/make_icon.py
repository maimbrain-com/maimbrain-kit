#!/usr/bin/env python3
"""Makes icon.png (256×256) from a preview screenshot (stdlib only): crops a
square around (cx, cy) of side `size` and box-filters it down.

    # in the preview: await mb.screenshot("icon-src")
    python3 tools/make_icon.py screenshots/icon-src.png icon.png 360 760 560
"""
import struct, sys, zlib


def read(path):
    data = open(path, "rb").read()
    pos, idat = 8, b""
    while pos < len(data):
        n, t = struct.unpack(">I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + n]
        if t == b"IHDR":
            w, h, _, ctype = struct.unpack(">IIBB", body[:10])
            ch = {6: 4, 2: 3}[ctype]
        elif t == b"IDAT":
            idat += body
        pos += 12 + n
    raw, stride = zlib.decompress(idat), w * ch
    out, prev = bytearray(h * stride), bytearray(stride)
    for y in range(h):
        f = raw[y * (stride + 1)]
        line = bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        for x in range(stride):
            a = line[x - ch] if x >= ch else 0
            b, c = prev[x], (prev[x - ch] if x >= ch else 0)
            if f == 1:
                line[x] = (line[x] + a) & 255
            elif f == 2:
                line[x] = (line[x] + b) & 255
            elif f == 3:
                line[x] = (line[x] + (a + b) // 2) & 255
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[x] = (line[x] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        out[y * stride:(y + 1) * stride] = line
        prev = line
    return w, h, ch, out


def write(path, n, rows):
    raw = b"".join(b"\x00" + r for r in rows)
    chunk = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    open(path, "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", n, n, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


src, dst, cx, cy, size = sys.argv[1], sys.argv[2], *map(int, sys.argv[3:6])
w, h, ch, px = read(src)
N = 256
rows = []
for y in range(N):
    row = bytearray()
    for x in range(N):
        acc, cnt = [0, 0, 0], 0
        y0, y1 = cy - size // 2 + y * size // N, cy - size // 2 + (y + 1) * size // N
        x0, x1 = cx - size // 2 + x * size // N, cx - size // 2 + (x + 1) * size // N
        for yy in range(max(y0, 0), min(max(y1, y0 + 1), h)):
            for xx in range(max(x0, 0), min(max(x1, x0 + 1), w)):
                i = (yy * w + xx) * ch
                for k in range(3):
                    acc[k] += px[i + k]
                cnt += 1
        row += bytes(a // max(cnt, 1) for a in acc)
    rows.append(bytes(row))
write(dst, N, rows)
print(f"wrote {dst}")
