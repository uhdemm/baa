"""Synthesize Baa's UI sounds (pure Python, no samples): soft, short, quiet.

    python -I scripts/make_sounds.py   -> public/sounds/*.wav
"""
import math
import pathlib
import struct
import wave

OUT = pathlib.Path(__file__).resolve().parent.parent / "public" / "sounds"
RATE = 44100


def tone(segments, volume=0.32):
    """segments: [(start_hz, end_hz, seconds)], each with a soft attack and decay."""
    samples = []
    phase = 0.0
    for f0, f1, dur in segments:
        n = int(RATE * dur)
        for i in range(n):
            t = i / n
            f = f0 + (f1 - f0) * t
            phase += 2 * math.pi * f / RATE
            env = min(1.0, i / (RATE * 0.006)) * (1 - t) ** 2.2  # click-free attack, round decay
            # a touch of the octave makes it rounder than a pure sine
            s = math.sin(phase) + 0.18 * math.sin(2 * phase)
            samples.append(s * env * volume)
    return samples


def write(name, samples):
    OUT.mkdir(parents=True, exist_ok=True)
    with wave.open(str(OUT / f"{name}.wav"), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(b"".join(struct.pack("<h", int(max(-1, min(1, s)) * 32767)) for s in samples))
    print("wrote", name)


write("start", tone([(660, 700, 0.07), (880, 940, 0.11)]))
write("stop", tone([(880, 840, 0.07), (620, 560, 0.12)]))
write("send", tone([(990, 1320, 0.09)], volume=0.26))
write("error", tone([(300, 260, 0.12), (300, 240, 0.16)], volume=0.3))
write("cancel", tone([(520, 380, 0.12)], volume=0.24))
