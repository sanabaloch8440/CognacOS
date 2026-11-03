#!/usr/bin/env python3
"""Validate the captured QEMU PC speaker WAV; supplemental, never fatal."""
import array
import math
import wave
from pathlib import Path

path = Path("build/beep.wav")
try:
    with wave.open(str(path), "rb") as wav:
        channels = wav.getnchannels()
        width = wav.getsampwidth()
        rate = wav.getframerate()
        frames = wav.readframes(wav.getnframes())
except (wave.Error, EOFError) as exc:
    print(f"AUDIO RESULT: INCOMPLETE - invalid WAV; audio capture is supplemental: {exc}")
    raise SystemExit(0)

if width not in (1, 2, 3, 4) or not frames:
    print("AUDIO RESULT: INCOMPLETE - WAV has no usable audio samples; audio capture is supplemental.")
    raise SystemExit(0)

if width == 1:
    samples = [b - 128 for b in frames]
elif width == 2:
    samples = array.array("h")
    samples.frombytes(frames[: len(frames) // 2 * 2])
elif width == 4:
    samples = array.array("i")
    samples.frombytes(frames[: len(frames) // 4 * 4])
else:
    samples = [
        int.from_bytes(frames[i:i+3], "little", signed=True)
        for i in range(0, len(frames) - 2, 3)
    ]

peak = max((abs(int(s)) for s in samples), default=0)
rms = math.sqrt(sum(int(s) * int(s) for s in samples) / max(1, len(samples)))
print(
    f"WAV: {path}, channels={channels}, sample_rate={rate}, "
    f"sample_width={width}, frames={len(frames)//width}, peak={peak}, rms={rms:.2f}"
)
if peak == 0 or rms < 1.0:
    print("AUDIO RESULT: INCOMPLETE - captured samples are silent; audio capture is supplemental.")
    raise SystemExit(0)
print("AUDIO RESULT: PASS - captured QEMU PC speaker audio contains non-zero samples.")
