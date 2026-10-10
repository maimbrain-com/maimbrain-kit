#!/usr/bin/env python3
"""Bake a color grade into a 3D LUT strip for `gfx3d::Grade { lut, .. }` (mb3d 2).

A LUT strip is an N²×N PNG (N = 2–64; 32 is plenty): N slices of blue side by
side, red across each slice, green down. The host looks every pixel's
graded color up in it, after the rest of the `Grade`, so anything a photo
editor can do to colors (curves, selective color, a film emulation preset)
becomes the game's last grading step.

  python3 tools/lut.py neutral assets/grade.png [--size 32]
      The identity strip: a LUT that changes nothing.
  python3 tools/lut.py stamp screenshots/shot-10s.png graded-me.png [--size 32]
      Pastes the identity LUT into the top-left corner of a screenshot
      (`mb shot`), its N slices in a compact grid (8 × 4 of 32×32 for N = 32). Grade that image in any editor until the game looks right,
      using only color adjustments (no blur, sharpening, vignettes or local
      edits), export it as PNG at the same size, then:
  python3 tools/lut.py extract graded-me.png assets/grade.png [--size 32]
      Cuts the grid back out into a strip: your grade, as a LUT.
  python3 tools/lut.py check assets/grade.png
      Says whether a PNG is a valid strip, and how far it is from neutral.

Then in the game: `Texture::new(Asset::load("assets/grade.png"))`, and
`gfx3d::grade(&Grade { lut: Some(texture), ..Grade::default() })`. Take the
screenshot with the beauty pass's grade *off* (or set the LUT's grade to
`Grade::default()` plus the LUT) so you don't grade twice.

Standard library only (zlib and struct); reads 8-bit RGB or RGBA,
non-interlaced PNGs (what editors export by default).
"""
from __future__ import annotations

import argparse
import struct
import sys
import zlib


def write_png(path: str, w: int, h: int, rows: list[bytes]) -> None:
    """rows: h rows of w*3 bytes (RGB)."""
    raw = b"".join(b"\x00" + r for r in rows)

    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)


def read_png(path: str) -> tuple[int, int, list[bytearray]]:
    """Returns (w, h, rows of w*3 RGB bytes)."""
    data = open(path, "rb").read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        sys.exit(f"{path}: not a PNG")
    pos, idat, w = 8, b"", 0
    while pos < len(data):
        n, kind = struct.unpack(">I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + n]
        if kind == b"IHDR":
            w, h, depth, ctype, _, _, interlace = struct.unpack(">IIBBBBB", body)
            if depth != 8 or ctype not in (2, 6) or interlace:
                sys.exit(f"{path}: need an 8-bit RGB or RGBA, non-interlaced PNG (got depth {depth}, color type {ctype}, interlace {interlace})")
            bpp = 3 if ctype == 2 else 4
        elif kind == b"IDAT":
            idat += body
        pos += 12 + n
    raw = zlib.decompress(idat)
    stride = w * bpp
    rows, prev, i = [], bytearray(stride), 0
    for _ in range(h):
        f, line = raw[i], bytearray(raw[i + 1:i + 1 + stride])
        i += 1 + stride
        for x in range(stride):
            a = line[x - bpp] if x >= bpp else 0
            b = prev[x]
            c = prev[x - bpp] if x >= bpp else 0
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
        prev = line
        rows.append(bytearray(b for k in range(w) for b in line[k * bpp:k * bpp + 3]))
    return w, h, rows


def neutral_rows(n: int) -> list[bytes]:
    v = lambda k: round(k * 255 / (n - 1))
    return [bytes(c for b in range(n) for r in range(n) for c in (v(r), v(g), v(b))) for g in range(n)]


def grid(n: int) -> tuple[int, int]:
    """Columns and rows of slices when a LUT is stamped onto an image."""
    cols = 1
    while cols * cols < n:
        cols += 1
    return cols, (n + cols - 1) // cols


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("command", choices=["neutral", "stamp", "extract", "check"])
    ap.add_argument("paths", nargs="+")
    ap.add_argument("--size", type=int, default=32, help="N: the LUT is N×N×N, the strip N²×N (2–64; default 32)")
    a = ap.parse_args()
    n = a.size
    if not 2 <= n <= 64:
        sys.exit("--size must be 2–64")
    if a.command == "neutral":
        write_png(a.paths[0], n * n, n, neutral_rows(n))
    elif a.command in ("stamp", "extract"):
        w, h, rows = read_png(a.paths[0])
        cols, nrows = grid(n)
        if w < cols * n or h < nrows * n:
            sys.exit(f"the image is smaller than the {cols * n}×{nrows * n} grid of slices")
        strip = neutral_rows(n) if a.command == "stamp" else [bytearray(n * n * 3) for _ in range(n)]
        for b in range(n):
            x0, y0 = (b % cols) * n * 3, (b // cols) * n
            for g in range(n):
                if a.command == "stamp":
                    rows[y0 + g][x0:x0 + n * 3] = strip[g][b * n * 3:(b + 1) * n * 3]
                else:
                    strip[g][b * n * 3:(b + 1) * n * 3] = rows[y0 + g][x0:x0 + n * 3]
        if a.command == "stamp":
            write_png(a.paths[1], w, h, [bytes(r) for r in rows])
        else:
            write_png(a.paths[1], n * n, n, [bytes(r) for r in strip])
    else:
        w, h, rows = read_png(a.paths[0])
        if w != h * h or not 2 <= h <= 64:
            sys.exit(f"{a.paths[0]}: {w}×{h} isn't an N²×N strip with N 2–64")
        ref = neutral_rows(h)
        diff = max(abs(x - y) for r, q in zip(rows, ref) for x, y in zip(r, q))
        print(f"{a.paths[0]}: a {h}³ LUT; it moves a channel by up to {diff} of 255")
    if a.command != "check":
        print(a.paths[-1])


if __name__ == "__main__":
    main()
