#!/usr/bin/env python3
"""Draw the Umiak app icons: the Open Loop, Free Computation's mark.

A thick ring, broken open at the top right, one square pixel outside
the gap, one dot at the center. Pure standard library on purpose: the
icons are generated artifacts (git-ignored), reproducible anywhere
python3 runs:

    python3 tools/make-icons.py

Output: app/src-tauri/icons/{icon.png,128x128.png,128x128@2x.png,32x32.png}
"""

import math
import os
import struct
import zlib

OUT = os.path.join(os.path.dirname(__file__), "..", "app", "src-tauri", "icons")


def draw(size):
    """RGBA pixel field for the Open Loop at the given square size."""
    px = bytearray(size * size * 4)
    c = size / 2.0
    ring_r = 0.345 * size        # ring centerline radius
    ring_half = 0.058 * size     # half the ring's thickness
    gap_dir = -math.pi / 4       # the break: top right
    gap_half = 0.30              # half the break's angle, radians
    dot_r = 0.048 * size
    sq_half = 0.030 * size
    sq_dist = 0.475 * size
    sq_x = c + sq_dist * math.cos(gap_dir)
    sq_y = c + sq_dist * math.sin(gap_dir)

    def black(x, y, a=255):
        i = (y * size + x) * 4
        px[i:i + 4] = bytes((0, 0, 0, a))

    for y in range(size):
        for x in range(size):
            dx, dy = x - c + 0.5, y - c + 0.5
            dist = math.hypot(dx, dy)
            ang = math.atan2(dy, dx)
            dang = abs((ang - gap_dir + math.pi) % (2 * math.pi) - math.pi)
            if abs(dist - ring_r) <= ring_half and dang > gap_half:
                black(x, y)
            elif dist <= dot_r:
                black(x, y)
            elif abs(x - sq_x + 0.5) <= sq_half and abs(y - sq_y + 0.5) <= sq_half:
                black(x, y)
    return px


def write_png(path, size, px):
    def chunk(kind, data):
        out = struct.pack(">I", len(data)) + kind + data
        out += struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
        return out

    raw = b""
    stride = size * 4
    for y in range(size):
        raw += b"\x00" + bytes(px[y * stride:(y + 1) * stride])
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)


def main():
    os.makedirs(OUT, exist_ok=True)
    for name, size in [("icon.png", 512), ("128x128@2x.png", 256),
                       ("128x128.png", 128), ("32x32.png", 32)]:
        write_png(os.path.join(OUT, name), size, draw(size))
        print("wrote", os.path.join(OUT, name))


if __name__ == "__main__":
    main()
