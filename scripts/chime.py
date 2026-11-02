#!/usr/bin/env python3
"""Make drift's sound for new messages (assets/sounds/message.wav): two soft,
rising notes with a few overtones, like a small marimba. Run it again after
changing the notes."""

import math
import struct
import wave
from pathlib import Path

RATE = 44100
LENGTH = 0.75
# (start in seconds, frequency in Hz, loudness)
NOTES = [(0.0, 880.0, 0.55), (0.11, 1318.5, 0.45)]
# Overtones: (multiple of the note, loudness, how much faster it fades)
OVERTONES = [(1.0, 1.0, 1.0), (2.0, 0.25, 1.8), (3.0, 0.08, 2.6)]
DECAY = 0.16
ATTACK = 0.004


def sample(t: float) -> float:
    total = 0.0
    for start, freq, loud in NOTES:
        dt = t - start
        if dt < 0:
            continue
        rise = min(1.0, dt / ATTACK)
        for mult, part, speed in OVERTONES:
            total += loud * part * rise * math.exp(-dt * speed / DECAY) * math.sin(2 * math.pi * freq * mult * dt)
    return total


def main() -> None:
    frames = [sample(i / RATE) for i in range(int(RATE * LENGTH))]
    peak = max(abs(f) for f in frames)
    scale = 0.6 * 32767 / peak
    out = Path(__file__).resolve().parent.parent / "assets" / "sounds" / "message.wav"
    with wave.open(str(out), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(b"".join(struct.pack("<h", int(f * scale)) for f in frames))
    print(out)


if __name__ == "__main__":
    main()
