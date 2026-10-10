#!/usr/bin/env python3
"""Locks a second music loop onto the first so the two can play as stems.

`mb music` cuts each track to whole bars on its own, so two tracks asked for
at the same tempo come out a few milliseconds apart in length and start at
different points of the beat. This stretches the second loop to exactly the
first's length, or a whole fraction of it (a 4-bar layer under an 8-bar loop
plays twice per loop; the stretch is a fraction of a percent: inaudible), and
rotates it (a seamless loop stays seamless from any start point) to where its
onsets line up best with the first's. Needs `mb ogg` (or oggenc/oggdec); stdlib only.

    python3 tools/align_loop.py assets/music.ogg assets/tense.ogg [-q 3]
"""

import array
import math
import os
import shutil
import subprocess
import sys
import tempfile
import wave

_MB = None  # the mb that has `mb ogg` ("" if there isn't one), found on first use


def _ogg(src: str, dst: str, quality: float = 5) -> None:
    """WAV -> mono Ogg Vorbis (dst ends in .ogg) or Ogg -> 16-bit WAV, with
    `mb ogg` (built into mb, on every OS) or else vorbis-tools' oggenc/oggdec."""
    global _MB
    if _MB is None:
        mb = shutil.which("mb")
        _MB = mb if mb and subprocess.run([mb, "ogg", "--help"], capture_output=True).returncode == 0 else ""
    enc = dst.lower().endswith(".ogg")
    if _MB:
        cmd = [_MB, "ogg", "-Q", "-q", str(quality), src, dst]
    elif shutil.which("oggenc" if enc else "oggdec"):
        cmd = ["oggenc", "-Q", "-q", str(quality), "-o", dst, src] if enc else ["oggdec", "-Q", "-o", dst, src]
    else:
        raise SystemExit("no Ogg Vorbis tool: update mb, which has `mb ogg` built in "
                         "(curl -fsSL https://maimbrain.com/install.sh | sh; on Windows irm https://maimbrain.com/install.ps1 | iex)")
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode:
        raise SystemExit(f"{' '.join(cmd)}: {r.stderr.strip()}")


def read(path):
    with tempfile.TemporaryDirectory() as d:
        wav = os.path.join(d, "a.wav")
        _ogg(path, wav)
        with wave.open(wav) as w:
            assert w.getsampwidth() == 2 and w.getnchannels() == 1, "expects mono 16-bit"
            rate = w.getframerate()
            s = array.array("h", w.readframes(w.getnframes()))
    return rate, [v / 32768.0 for v in s]


def write(path, rate, samples, quality):
    with tempfile.TemporaryDirectory() as d:
        wav = os.path.join(d, "a.wav")
        with wave.open(wav, "wb") as w:
            w.setnchannels(1)
            w.setsampwidth(2)
            w.setframerate(rate)
            w.writeframes(array.array("h", (max(-32767, min(32767, round(v * 32767))) for v in samples)).tobytes())
        _ogg(wav, path, quality)


def onsets(s, hop):
    # Positive change of loudness per hop: where notes and hits start.
    env = []
    for i in range(0, len(s) - hop + 1, hop):
        e = math.sqrt(sum(v * v for v in s[i : i + hop]) / hop)
        env.append(e)
    return [max(0.0, env[i] - env[i - 1]) for i in range(len(env))]


def main():
    args = sys.argv[1:]
    q = 3
    if "-q" in args:
        i = args.index("-q")
        q = args[i + 1]
        del args[i : i + 2]
    lead, follow = args
    rate, a = read(lead)
    rate_b, b = read(follow)
    assert rate == rate_b, "both loops need the same sample rate"
    reps = max(1, round(len(a) / len(b)))
    n = len(a) // reps
    # Stretch: linear interpolation over the loop (wrapping at the seam).
    k = len(b) / n
    b = [b[int(i * k) % len(b)] * (1 - (i * k) % 1) + b[(int(i * k) + 1) % len(b)] * ((i * k) % 1) for i in range(n)]
    hop = 256
    # The lead's onsets folded onto the follower's length.
    ea = onsets(a[: n * reps], hop)
    per = len(ea) // reps
    ea = [sum(ea[i + r * per] for r in range(reps)) for i in range(per)]
    eb = onsets(b, hop)
    m = min(len(ea), len(eb))
    best, lag = -1.0, 0
    for l in range(m):
        c = sum(ea[i] * eb[(i + l) % m] for i in range(0, m))
        if c > best:
            best, lag = c, l
    shift = (lag * hop) % n
    b = b[shift:] + b[:shift]
    write(follow, rate, b, q)
    print(f"{follow}: stretched {k:.5f}x to {n / rate:.3f} s ({reps} per loop) and rotated {shift / rate * 1000:.0f} ms onto {lead}")


if __name__ == "__main__":
    main()
