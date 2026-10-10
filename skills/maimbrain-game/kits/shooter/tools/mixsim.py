#!/usr/bin/env python3
"""Renders a bot-played round's sound offline and measures the mix: how
loud the effects are against the music over time, and which sounds take
the most of it. The events come from the game's own sound code (the
`mix_trace` test logs every play, voice stop, loop start and track level
that `src/sound.rs` asks for), so this hears what the game would play.

    MIX_TRACE=/tmp/t.csv cargo test -p kit_shooter --release -- --ignored mix_trace
    python3 tools/mixsim.py /tmp/t.csv [--assets assets] [--wav /tmp/mix.wav]

(MIX_SEED and MIX_SKILL=first|decent|good pick the round.)

Loudness is K-weighted RMS (roughly what LUFS measures) over 400 ms
windows. "fx - music" is the effects' loudness minus the music's, window by
window, during play: the mix rule is that the frequent sounds stay several
dB under the music, so its median should be clearly negative.

Needs numpy, and `mb ogg` (or oggdec) to decode.
"""

import argparse
import csv
import os
import shutil
import subprocess
import sys
import tempfile
import wave

import numpy as np

SR = 32000

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


def load(path):
    with tempfile.TemporaryDirectory() as d:
        w = os.path.join(d, "a.wav")
        _ogg(path, w)
        with wave.open(w) as r:
            n, ch, sr = r.getnframes(), r.getnchannels(), r.getframerate()
            a = np.frombuffer(r.readframes(n), dtype="<i2").astype(np.float64) / 32768
    if ch > 1:
        a = a.reshape(-1, ch).mean(1)
    if sr != SR:
        a = np.interp(np.arange(int(len(a) * SR / sr)) * sr / SR, np.arange(len(a)), a)
    return a


def kweight(x):
    """A close-enough K-weighting: high-pass ~60 Hz, +4 dB shelf above ~1.7 kHz."""
    X = np.fft.rfft(x)
    f = np.fft.rfftfreq(len(x), 1 / SR)
    hp = (f / 60) ** 2 / (1 + (f / 60) ** 2)
    shelf = 1 + (10 ** (4 / 20) - 1) * (f / 1700) ** 2 / (1 + (f / 1700) ** 2)
    return np.fft.irfft(X * hp * shelf, len(x))


def db(v):
    return 20 * np.log10(np.maximum(v, 1e-9))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("trace")
    ap.add_argument("--assets", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets"))
    ap.add_argument("--wav")
    ap.add_argument("--quiet", action="store_true")
    a = ap.parse_args()

    rows = list(csv.DictReader(open(a.trace)))
    end = max(float(r["t"]) for r in rows) + 3.0
    n = int(end * SR)
    cache = {}

    def snd(name):
        if name not in cache:
            cache[name] = load(os.path.join(a.assets, name + ".ogg"))
        return cache[name]

    tracks = {}  # loops: name -> list of (t, kind, vol)
    voices = {}  # voice id -> (name, start sample, vol, pitch)
    stops = {}
    fx = {}  # per sound name: rendered buffer
    count = {}
    t_over = None
    for r in rows:
        t, k, name, vol, pitch = float(r["t"]), int(r["kind"]), r["sound"], float(r["vol"]), float(r["pitch"])
        vid = int(float(r.get("voice", 0) or 0))
        if k in (1, 2):
            tracks.setdefault(name, []).append((t, k, vol))
        elif k == 0:
            voices[len(voices) if not vid else vid] = (name, t, vol, pitch)
            count[name] = count.get(name, 0) + 1
        elif k == 3:
            stops[vid] = t
        if name in ("over", "best") and k == 0:
            t_over = t if t_over is None else min(t_over, t)

    for vid, (name, t, vol, pitch) in voices.items():
        s = snd(name)
        L = int(len(s) / pitch)
        if vid in stops:
            L = min(L, max(0, int((stops[vid] - t) * SR)))
        y = np.interp(np.arange(L) * pitch, np.arange(len(s)), s) * vol
        if vid in stops and L > 64:
            y[-64:] *= np.linspace(1, 0, 64)
        i = int(t * SR)
        b = fx.setdefault(name, np.zeros(n))
        y = y[: n - i]
        b[i : i + len(y)] += y

    music = np.zeros(n)
    loops = {}
    for name, evs in tracks.items():
        s = snd(name)
        gain = np.zeros(n)
        phase0 = None
        lv_t, lv_v = [], []
        starts = []
        for t, k, vol in evs:
            if k == 1:
                starts.append(t)
            else:
                lv_t.append(t)
                lv_v.append(vol)
        if not starts:
            continue
        g = np.interp(np.arange(n) / SR, lv_t, lv_v, left=0, right=0) if lv_t else np.zeros(n)
        y = np.zeros(n)
        bounds = starts + [end]
        for j, st in enumerate(starts):
            i0, i1 = int(st * SR), min(n, int(bounds[j + 1] * SR))
            if i1 > i0:
                idx = np.arange(i1 - i0) % len(s)
                y[i0:i1] = s[idx]
        y *= g
        if name.startswith("music"):
            music += y
        else:
            loops[name] = y
    for name, y in loops.items():
        fx[name] = fx.get(name, np.zeros(n)) + y

    allfx = sum(fx.values()) if fx else np.zeros(n)
    if a.wav:
        mix = music + allfx
        pk = np.abs(mix).max()
        with wave.open(a.wav, "wb") as w:
            w.setnchannels(1)
            w.setsampwidth(2)
            w.setframerate(SR)
            w.writeframes((np.clip(mix, -1, 1) * 32767).astype("<i2").tobytes())
        print(f"wrote {a.wav} (peak {db(pk):.1f} dBFS{', CLIPS' if pk > 1 else ''})")

    play_end = t_over if t_over else end
    win = int(0.4 * SR)
    km, kf = kweight(music), kweight(allfx)
    kparts = {k: kweight(v) for k, v in fx.items()}
    W = int(play_end * SR) // win
    rms = lambda x: np.sqrt(np.mean(x[: W * win].reshape(W, win) ** 2, axis=1))
    m, f = rms(km), rms(kf)
    live = m > 1e-4
    diff = db(f[live]) - db(m[live])
    print(f"round {play_end:.1f} s, music {db(np.sqrt(np.mean(km[: W * win] ** 2))):.1f} dB (K-weighted RMS)")
    print(
        f"fx - music (400 ms windows while music plays): median {np.median(diff):+.1f} dB, p90 {np.percentile(diff, 90):+.1f}, "
        f"max {diff.max():+.1f}; fx louder than music in {100 * np.mean(diff > 0):.0f}% of windows, within 3 dB in {100 * np.mean(diff > -3):.0f}%"
    )
    freq = [k for k in fx if count.get(k, 0) / play_end * 60 > 60]
    if freq:
        kq = sum(kparts[k] for k in freq)
        d2 = db(rms(kq)[live]) - db(m[live])
        print(
            f"frequent sounds ({', '.join(sorted(freq))}; >1/s) - music: median {np.median(d2):+.1f} dB, "
            f"p90 {np.percentile(d2, 90):+.1f}, max {d2.max():+.1f}"
        )
    mix = music + allfx
    pk = np.abs(mix).max()
    hot = np.argsort(-np.abs(mix[:: SR // 100]))[:200] / 100
    spots = sorted({round(float(t), 1) for t in hot})[:6]
    print(f"mix peak {db(pk):.1f} dBFS (loudest moments near {', '.join(f'{t:.1f}' for t in spots)} s); summed fx peak {db(np.abs(allfx).max()):.1f}")
    tot = sum(float(np.sum(v[: W * win] ** 2)) for v in kparts.values()) or 1
    print(f"{'sound':12} {'plays':>6} {'/min':>6} {'share':>6} {'loud(dB)':>9} {'p90 vs music':>13}")
    order = sorted(kparts, key=lambda k: -float(np.sum(kparts[k][: W * win] ** 2)))
    for k in order:
        e = float(np.sum(kparts[k][: W * win] ** 2))
        r = rms(kparts[k])
        act = r > 1e-4
        loud = db(np.sqrt(np.mean(r[act] ** 2))) if act.any() else -99
        rel = np.percentile(db(r[live & act]) - db(m[live & act]), 90) if (live & act).any() else float("nan")
        c = count.get(k, 0)
        print(f"{k:12} {c:6d} {c / play_end * 60:6.0f} {100 * e / tot:5.0f}% {loud:9.1f} {rel:+13.1f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
