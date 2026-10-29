#!/usr/bin/env python3
"""Draws Samo's app icon and writes `apps/samo-gpui/assets/Samo.icns` (plus a 1024 px PNG for the docs).

The icon is Samo's mark on its instrument face: an Ink squircle with the measuring grid, a gauge ring open at
the bottom, the measured arc in pale Cobalt, the part Samo suggests clearing in dither, and the needle.

Needs Pillow (`python3 -m pip install pillow`) and macOS's `iconutil`. Run it from anywhere:

    python3 scripts/make-icon.py
"""

import math
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
ICNS = ROOT / "apps/samo-gpui/assets/Samo.icns"
PNG = ROOT / "docs/media/samo-icon.png"

S = 4  # supersampling
W = 1024 * S

INK_TOP = (44, 51, 64)
INK_BOTTOM = (20, 24, 30)
PALE_COBALT = (171, 192, 239)
WHITE = (246, 248, 252)


def arc_points(cx, cy, r, a0, a1, steps):
    return [
        (cx + r * math.cos(a0 + (a1 - a0) * i / steps), cy + r * math.sin(a0 + (a1 - a0) * i / steps))
        for i in range(steps + 1)
    ]


def band(cx, cy, r, width, a0, a1):
    """A ring segment as a polygon."""
    outer = arc_points(cx, cy, r + width / 2, a0, a1, 180)
    inner = arc_points(cx, cy, r - width / 2, a1, a0, 180)
    return outer + inner


def draw():
    img = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    cx = cy = W / 2
    half = 412 * S  # the 824 px body of Apple's 1024 px template

    body = [cx - half, cy - half, cx + half, cy + half]
    radius = 185 * S

    # A soft shadow under the body.
    shadow = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rounded_rectangle(
        [body[0], body[1] + 14 * S, body[2], body[3] + 14 * S], radius, fill=(0, 0, 0, 110)
    )
    img.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(18 * S)))

    # The face: Ink, a little lighter at the top.
    face = Image.new("RGBA", (W, W))
    fd = ImageDraw.Draw(face)
    for y in range(W):
        t = y / W
        fd.line(
            [(0, y), (W, y)],
            fill=tuple(round(INK_TOP[k] * (1 - t) + INK_BOTTOM[k] * t) for k in range(3)) + (255,),
        )
    mask = Image.new("L", (W, W), 0)
    ImageDraw.Draw(mask).rounded_rectangle(body, radius, fill=255)

    # The measuring grid: dots on a lattice, every fourth stronger, fading downward.
    grid = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    gd = ImageDraw.Draw(grid)
    step = 40 * S
    for j in range(int(W / step) + 1):
        y = j * step
        fade = max(0.0, 1.0 - y / (W * 0.62))
        for i in range(int(W / step) + 1):
            x = i * step
            major = i % 4 == 0 and j % 4 == 0
            a = round((70 if major else 34) * fade)
            rr = (4 if major else 3) * S
            gd.ellipse([x - rr, y - rr, x + rr, y + rr], fill=PALE_COBALT + (a,))
    face.alpha_composite(grid)
    img.paste(face, (0, 0), mask)

    # Translucent marks go on their own layer: drawing them straight onto the face would punch holes in it.
    glow = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    g = ImageDraw.Draw(glow)
    d = ImageDraw.Draw(img)
    gx, gy = cx, cy + 34 * S
    r = 262 * S
    width = 64 * S
    a0, a1 = 0.75 * math.pi, 2.25 * math.pi  # open at the bottom
    span = a1 - a0
    measured, suggested = 0.58, 0.16  # shares of the ring

    # Twelve ticks around the ring, like the page dials.
    for i in range(12):
        a = i / 12 * 2 * math.pi
        r0, r1 = r + width / 2 + 34 * S, r + width / 2 + 62 * S
        g.line(
            [(gx + r0 * math.cos(a), gy + r0 * math.sin(a)), (gx + r1 * math.cos(a), gy + r1 * math.sin(a))],
            fill=PALE_COBALT + (110,),
            width=10 * S,
        )

    # The track, the measured arc, then the suggested arc in dither.
    g.polygon(band(gx, gy, r, width, a0, a1), fill=WHITE + (30,))
    end = a1
    ex, ey = gx + r * math.cos(end), gy + r * math.sin(end)
    g.ellipse([ex - width / 2, ey - width / 2, ex + width / 2, ey + width / 2], fill=WHITE + (30,))
    img.alpha_composite(glow)
    m_end = a0 + span * measured
    d.polygon(band(gx, gy, r, width, a0, m_end), fill=PALE_COBALT + (255,))
    s_end = m_end + span * suggested
    dither = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    dd = ImageDraw.Draw(dither)
    cell = 13 * S
    for j in range(int(W / cell)):
        for i in range(int(W / cell)):
            if (i + j) % 2 == 0:
                dd.rectangle([i * cell, j * cell, (i + 1) * cell - 1, (j + 1) * cell - 1], fill=PALE_COBALT + (255,))
    seg = Image.new("L", (W, W), 0)
    ImageDraw.Draw(seg).polygon(band(gx, gy, r, width, m_end, s_end), fill=255)
    img.paste(dither, (0, 0), Image.composite(dither.split()[3], Image.new("L", (W, W), 0), seg))

    # Round caps on the measured arc's start.
    for a in (a0,):
        px_, py_ = gx + r * math.cos(a), gy + r * math.sin(a)
        d.ellipse([px_ - width / 2, py_ - width / 2, px_ + width / 2, py_ + width / 2], fill=PALE_COBALT + (255,))

    # The needle and its hub.
    needle = m_end
    tip = (gx + r * 0.74 * math.cos(needle), gy + r * 0.74 * math.sin(needle))
    d.line([(gx, gy), tip], fill=WHITE + (255,), width=34 * S)
    for (x, y), rad in (((gx, gy), 52 * S), (tip, 17 * S)):
        d.ellipse([x - rad, y - rad, x + rad, y + rad], fill=WHITE + (255,))
    d.ellipse([gx - 20 * S, gy - 20 * S, gx + 20 * S, gy + 20 * S], fill=INK_BOTTOM + (255,))

    return img.resize((1024, 1024), Image.LANCZOS)


def main():
    if shutil.which("iconutil") is None:
        sys.exit("iconutil not found: run this on macOS")
    icon = draw()
    PNG.parent.mkdir(parents=True, exist_ok=True)
    icon.save(PNG)
    with tempfile.TemporaryDirectory() as tmp:
        iconset = Path(tmp) / "Samo.iconset"
        iconset.mkdir()
        for size in (16, 32, 128, 256, 512):
            icon.resize((size, size), Image.LANCZOS).save(iconset / f"icon_{size}x{size}.png")
            icon.resize((size * 2, size * 2), Image.LANCZOS).save(iconset / f"icon_{size}x{size}@2x.png")
        subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", str(ICNS)], check=True)
    print(f"wrote {ICNS.relative_to(ROOT)} and {PNG.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
