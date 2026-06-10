#!/usr/bin/env python3
"""Generate the in-game console skin (assets/hud/*.png), pure stdlib.

Three gunmetal panels for the inverted-T HUD (see apps/client/src/hud.rs):
a left wing (the minimap housing), a right wing (the command card), and a
wide centre console strip that the HUD stretches between them. Styled after
the classic StarCraft console: brushed metal, bevelled edges, rivets, and a
cool glow trim along the top edge.

Run: python3 assets/gen_hud_skin.py
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "worldgen"))
from common import write_png, value_noise, clamp8  # noqa: E402

OUT = os.path.join(os.path.dirname(__file__), "hud")


def brushed(px, w, h, seed, base=(22, 30, 46), depth=10):
    """Fill with a vertical-gradient gunmetal + horizontal brushed streaks."""
    for y in range(h):
        t = y / max(h - 1, 1)
        shade = 1.08 - 0.34 * t  # lit at the top, falling into shadow
        streak = (value_noise(0.0, y * 0.9, seed) - 0.5) * depth
        for x in range(w):
            grain = (value_noise(x * 0.16, y * 0.9, seed + 7) - 0.5) * 6.0
            v = shade + (streak + grain) / 255.0
            i = (y * w + x) * 3
            px[i] = clamp8(base[0] * v)
            px[i + 1] = clamp8(base[1] * v)
            px[i + 2] = clamp8(base[2] * v)


def blend(px, w, x, y, col, a):
    if a <= 0.0:
        return
    i = (y * w + x) * 3
    ia = 1.0 - a
    px[i] = clamp8(px[i] * ia + col[0] * a)
    px[i + 1] = clamp8(px[i + 1] * ia + col[1] * a)
    px[i + 2] = clamp8(px[i + 2] * ia + col[2] * a)


def hline(px, w, h, y, col, a=1.0):
    if 0 <= y < h:
        for x in range(w):
            blend(px, w, x, y, col, a)


def vline(px, w, h, x, col, a=1.0):
    if 0 <= x < w:
        for y in range(h):
            blend(px, w, x, y, col, a)


def trim(px, w, h):
    """Bevel + glow trim shared by every panel: bright top lip over a cyan
    accent line, darker bottom edge."""
    hline(px, w, h, 0, (150, 190, 235), 0.9)
    hline(px, w, h, 1, (90, 120, 160), 0.7)
    hline(px, w, h, 2, (60, 190, 235), 0.85)  # the glow line
    hline(px, w, h, 3, (32, 90, 120), 0.5)
    hline(px, w, h, h - 2, (6, 9, 16), 0.7)
    hline(px, w, h, h - 1, (4, 6, 12), 0.9)


def rivet(px, w, h, cx, cy):
    """A small domed rivet with a top-left catchlight."""
    for dy in range(-2, 3):
        for dx in range(-2, 3):
            d = math.hypot(dx, dy)
            if d > 2.4:
                continue
            x, y = cx + dx, cy + dy
            if not (0 <= x < w and 0 <= y < h):
                continue
            lit = 0.5 - (dx + dy) * 0.18 - d * 0.1
            blend(px, w, x, y, (170, 195, 225), max(0.15, min(0.85, lit)))
    blend(px, w, cx - 1, cy - 1, (235, 245, 255), 0.9)


def plate_seams(px, w, h, seed, step=86):
    """Vertical panel seams with rivets, so long strips read as plating."""
    x = step
    while x < w - 20:
        vline(px, w, h, x, (10, 14, 24), 0.55)
        vline(px, w, h, x + 1, (70, 92, 124), 0.35)
        rivet(px, w, h, x, 10)
        rivet(px, w, h, x, h - 10)
        x += step


def corner_gussets(px, w, h):
    """Diagonal corner plates (the SC-style angled cuts) on the top corners."""
    for k in range(26):
        a = 0.5 - k * 0.012
        for t in range(2):
            y = k + t
            if y < h:
                blend(px, w, 25 - k, y, (110, 140, 180), a)
                blend(px, w, w - 26 + k, y, (110, 140, 180), a)
    rivet(px, w, h, 8, h - 10)
    rivet(px, w, h, w - 9, h - 10)


def warning_chevrons(px, w, h, y0, span):
    """A faint hazard-chevron band, very low contrast (texture, not text)."""
    for y in range(y0, min(y0 + 6, h)):
        for x in range(10, w - 10):
            if (x + y) % span < span // 2:
                blend(px, w, x, y, (96, 84, 36), 0.16)


def panel(w, h, seed, chevrons=False):
    px = bytearray(w * h * 3)
    brushed(px, w, h, seed)
    plate_seams(px, w, h, seed)
    trim(px, w, h)
    corner_gussets(px, w, h)
    if chevrons:
        warning_chevrons(px, w, h, h - 14, 16)
    return px


def main():
    os.makedirs(OUT, exist_ok=True)
    jobs = [
        ("wing_left.png", 184, 184, 11, False),
        ("wing_right.png", 308, 184, 23, False),
        ("console_mid.png", 1024, 124, 37, True),
    ]
    for name, w, h, seed, chev in jobs:
        write_png(os.path.join(OUT, name), w, h, panel(w, h, seed, chev))
        print(f"  {name} {w}x{h}")


if __name__ == "__main__":
    main()
