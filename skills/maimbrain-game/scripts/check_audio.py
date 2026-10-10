"""Checks generated audio without listening: length, peak, loudness,
brightness (a harshness proxy), how much of it a phone speaker loses and,
for loops, the jump at the loop point. Flags silent, clipping, harsh and
(for effects) bass-heavy files. You can't hear your sounds, so run this
after every audio change, and scripts/mix_check.py for how they sit
together in the game.

Brightness is the RMS frequency in Hz (a sine of f Hz reads f, white noise
a quarter of the sample rate), so it means the same at any sample rate;
above ~6500 Hz reads as harsh. "phone dB" is how much quieter the sound is
through a phone speaker than to the ear (mix_check.py's model: K-weighting,
and a 4th-order roll-off under 300 Hz), over an effect's loudest 50 ms or a
loop's whole length. Past -10 dB an effect is mostly bass a phone can't
play: `mb sfx` warns when it makes one, and `mb regen <file> --reprocess
--brighten` adds harmonics a phone can play, for free. Loops are printed,
not flagged (music with a bass line loses a few dB more). The loop jump
compares the step from the last sample to the first with the steps right
around the seam.

    python3 <skill dir>/scripts/check_audio.py games/<name>/assets/*.ogg
    python3 .../check_audio.py --loops music,wind games/<name>/assets/*.ogg

Files whose name contains "music" or "loop" (or any --loops word) are
checked as loops. Decodes with `mb ogg` (or oggdec, with an older mb).
"""

from __future__ import annotations

import math
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import wave

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
try:  # one phone model for both; mb new copies mix_check.py next to this file
    from mix_check import PEAK_WIN, PHONE_HP, biquad, k_filters
except ImportError:
    k_filters = None

PHONE_FLAG = -10.0  # dB: an effect losing more than this on a phone is flagged

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


def decode(path: str) -> tuple[list[float], int]:
    with tempfile.TemporaryDirectory() as d:
        out = os.path.join(d, "x.wav")
        _ogg(path, out)
        with wave.open(out) as w:
            n, rate, ch = w.getnframes(), w.getframerate(), w.getnchannels()
            raw = w.readframes(n)
    s = struct.unpack(f"<{n * ch}h", raw)[::ch]
    return [v / 32768 for v in s], rate


def db(x: float) -> float:
    return 20 * math.log10(max(x, 1e-9))


def phone_hp(rate: int) -> list[tuple]:
    """A 4th-order Butterworth high-pass at PHONE_HP, as two biquads."""
    w = 2 * math.pi * PHONE_HP / rate
    c = math.cos(w)
    out = []
    for q in (0.5411961, 1.3065630):
        al = math.sin(w) / (2 * q)
        out.append((((1 + c) / 2, -(1 + c), (1 + c) / 2), (1 + al, -2 * c, 1 - al)))
    return out


def phone_loss(x: list[float], rate: int, peak: bool) -> float:
    """dB of x a phone speaker loses as the ear hears it (<= 0), over its
    loudest PEAK_WIN (an effect: what mix_check.py measures of each play)
    or all of it (a loop)."""
    ear = x
    for b, a in k_filters(rate):
        ear = biquad(ear, b, a)
    ph = ear
    for b, a in phone_hp(rate):
        ph = biquad(ph, b, a)
    w = int(rate * PEAK_WIN)
    if peak and len(x) > w:
        ce, cp = [0.0], [0.0]
        for e, p in zip(ear, ph):
            ce.append(ce[-1] + e * e)
            cp.append(cp[-1] + p * p)
        j = max(range(0, len(x) - w + 1, max(1, w // 4)), key=lambda j: ce[j + w] - ce[j])
        e, p = ce[j + w] - ce[j], cp[j + w] - cp[j]
    else:
        e, p = sum(v * v for v in ear), sum(v * v for v in ph)
    return 10 * math.log10(max(p, 1e-30) / max(e, 1e-30))


def main(argv: list[str]) -> int:
    loops = {"music", "loop"}
    files = []
    it = iter(argv)
    for a in it:
        if a == "--loops":
            loops |= set(next(it).split(","))
        else:
            files.append(a)
    if not files:
        print(__doc__)
        return 2
    bad = 0
    print(f"{'file':22} {'secs':>6} {'peak dB':>8} {'rms dB':>7} {'bright Hz':>9} {'phone dB':>8} {'loop jump':>9}  notes")
    for p in sorted(files):
        x, rate = decode(p)
        name = os.path.basename(p)
        peak = max((abs(v) for v in x), default=0.0)
        loud = sorted(v * v for v in x)[len(x) // 3:]
        rms = math.sqrt(sum(loud) / max(1, len(loud)))
        full = math.sqrt(sum(v * v for v in x) / max(1, len(x))) or 1e-9
        diff = math.sqrt(sum((x[i] - x[i - 1]) ** 2 for i in range(1, len(x))) / max(1, len(x)))
        # rms(diff)/rms = 2 sin(pi f / rate) for a sine: invert it, so the
        # number doesn't depend on the sample rate.
        bright = rate / math.pi * math.asin(min(1.0, diff / full / 2))
        notes, jump = [], ""
        is_loop = any(w in name for w in loops)
        phone = phone_loss(x, rate, not is_loop) if peak >= 0.01 and k_filters else 0.0
        if is_loop:
            seam = abs(x[0] - x[-1])
            # The steps on either side of the seam, not the file's average
            # (a quiet intro made loud seams look like clicks).
            k = min(32, len(x) // 4)
            near = [abs(x[i] - x[i - 1]) for i in range(1, k + 1)] + [abs(x[-i] - x[-i - 1]) for i in range(1, k + 1)]
            ratio = seam / max(sum(near) / max(1, len(near)), 1e-4)
            jump = f"{ratio:8.1f}x"
            if ratio > 4 and seam > 0.003:
                notes.append("CLICK at loop seam")
        if peak < 0.01:
            notes.append("SILENT")
        elif db(peak) > -0.5:
            notes.append("clips (peak > -0.5 dB)")
        if peak >= 0.01 and db(rms) < -30 and not jump:  # stems are quiet by design
            notes.append("very quiet")
        if bright > 6500:
            notes.append("harsh/bright (try a lower cutoff)")
        if not is_loop and phone < PHONE_FLAG:
            notes.append(f"bassy: a phone loses {-phone:.0f} dB of it (a brighter take, an octave up, or mb regen <file> --reprocess --brighten)")
        bad += bool(notes)
        ph = f"{phone:8.1f}" if k_filters else f"{'-':>8}"
        print(f"{name:22} {len(x) / rate:6.2f} {db(peak):8.1f} {db(rms):7.1f} {bright:9.0f} {ph} {jump:>9}  {', '.join(notes)}")
    if not k_filters:
        print("\n(no phone column: put the skill's mix_check.py next to this file)")
    print(f"\n{bad} file(s) flagged" if bad else "\nall files look sane")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
