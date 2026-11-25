"""Decode one composited frame per request, keeping the decoder open between frames."""
import json
import re
import signal
import subprocess
import sys
from pathlib import Path
from PIL import Image, ImageOps

MAX_PIXELS = 64_000_000
Image.MAX_IMAGE_PIXELS = MAX_PIXELS
source = Path(sys.argv[1])
output = Path("frame.png")


def avif_reader():
    # Older Pillow releases use the system libavif tools for sequences.
    args = ["/usr/bin/avifdec", "-j", "2", "--size-limit", str(MAX_PIXELS)]
    info = subprocess.check_output([*args, "--info", str(source)], timeout=25, text=True)
    scale = int(re.search(r"(\d+) timescales per second", info)[1])
    durations = [int(t) * 1_000_000 // scale for t in re.findall(r"\[duration [\d.]+ \((\d+) timescales\)\]", info)]
    repeat = re.search(r"Repeat Count\s*:\s*(\S+)", info, re.I)
    repeats = -1 if repeat and "infinite" in repeat[1].lower() else int(repeat[1]) if repeat and repeat[1].isdigit() else 0

    def write(index):
        subprocess.run([*args, "--index", str(index), "--png-compress", "1", str(source), str(output)],
                       stdout=subprocess.DEVNULL, check=True, timeout=25)
        return durations[index]

    return len(durations), repeats, write


def pillow_reader():
    image = Image.open(source, formats=["GIF", "PNG", "WEBP"])
    if image.width * image.height > MAX_PIXELS:
        raise ValueError("The animation exceeds the playback size limit.")
    offset = int(bool(image.info.get("default_image", False)))
    count = image.n_frames - offset
    # GIF loop counts are repeats. APNG and WebP store total play counts.
    loops = image.info.get("loop")
    repeats = 0 if loops is None else -1 if loops == 0 else loops if image.format == "GIF" else loops - 1

    def write(index):
        image.seek(index + offset)
        frame = ImageOps.exif_transpose(image.convert("RGBA"))
        frame.save(output, format="PNG", compress_level=1)
        return round(float(image.info.get("duration", 100)) * 1000)

    return count, repeats, write


def run():
    signal.alarm(30)
    count, repeats, write = avif_reader() if sys.argv[2] == "AVIF" else pillow_reader()
    if not 1 < count <= 10000:
        raise ValueError("The frame count is outside the playback limit.")
    signal.alarm(0)
    for line in sys.stdin:
        signal.alarm(30)
        index = int(line)
        if not 0 <= index < count:
            raise ValueError("Invalid frame index.")
        duration = write(index)
        print(json.dumps([count, repeats, min(max(0, duration), 4294967295)]), flush=True)
        signal.alarm(0)


try:
    run()
except Exception as error:
    print(json.dumps(str(error)), flush=True)
    sys.exit(1)
