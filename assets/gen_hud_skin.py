#!/usr/bin/env python3
"""Generate the in-game console skin (assets/hud/*.png), pure stdlib.

Three panels for the inverted-T HUD (see apps/client/src/hud.rs): a left
wing (the minimap housing), a right wing (the command card), and a wide
centre console strip stretched between them. Styled as machined sci-fi
metal: a steel-blue base with a moving specular band and anisotropic
brushed grain, inset bevelled plates, chamfer cuts, vent grilles, glowing
cyan conduits with bloom, and status LEDs.

Run: python3 assets/gen_hud_skin.py
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "worldgen"))
from common import write_png, value_noise, clamp8  # noqa: E402

OUT = os.path.join(os.path.dirname(__file__), "hud")

BASE = (96, 108, 128)  # steel blue, lit value; shading multiplies this
GLOW = (70, 200, 245)


class Img:
    def __init__(self, w, h):
        self.w, self.h = w, h
        self.px = bytearray(w * h * 3)

    def get(self, x, y):
        i = (y * self.w + x) * 3
        return self.px[i], self.px[i + 1], self.px[i + 2]

    def put(self, x, y, c):
        if 0 <= x < self.w and 0 <= y < self.h:
            i = (y * self.w + x) * 3
            self.px[i] = clamp8(c[0])
            self.px[i + 1] = clamp8(c[1])
            self.px[i + 2] = clamp8(c[2])

    def blend(self, x, y, c, a):
        if a <= 0.0 or not (0 <= x < self.w and 0 <= y < self.h):
            return
        i = (y * self.w + x) * 3
        ia = 1.0 - a
        self.px[i] = clamp8(self.px[i] * ia + c[0] * a)
        self.px[i + 1] = clamp8(self.px[i + 1] * ia + c[1] * a)
        self.px[i + 2] = clamp8(self.px[i + 2] * ia + c[2] * a)

    def add(self, x, y, c, a):
        """Additive glow."""
        if a <= 0.0 or not (0 <= x < self.w and 0 <= y < self.h):
            return
        i = (y * self.w + x) * 3
        self.px[i] = clamp8(self.px[i] + c[0] * a)
        self.px[i + 1] = clamp8(self.px[i + 1] + c[1] * a)
        self.px[i + 2] = clamp8(self.px[i + 2] + c[2] * a)

    def mul(self, x, y, f):
        if 0 <= x < self.w and 0 <= y < self.h:
            i = (y * self.w + x) * 3
            self.px[i] = clamp8(self.px[i] * f)
            self.px[i + 1] = clamp8(self.px[i + 1] * f)
            self.px[i + 2] = clamp8(self.px[i + 2] * f)


def metal_base(img, seed):
    """Steel with a specular reflection band and brushed horizontal grain."""
    w, h = img.w, img.h
    for y in range(h):
        t = y / max(h - 1, 1)
        # Lighting curve: bright machined lip, a soft specular band in the
        # upper third (a light source reflecting off curved metal), falling
        # into shadow toward the bottom.
        shade = 0.78 - 0.42 * t
        shade += 0.55 * math.exp(-(((t - 0.16) / 0.10) ** 2))  # spec band
        shade += 0.10 * math.exp(-(((t - 0.55) / 0.25) ** 2))  # soft fill
        streak = (value_noise(0.0, y * 1.7, seed) - 0.5) * 0.16
        for x in range(w):
            grain = (value_noise(x * 0.05, y * 1.7, seed + 7) - 0.5) * 0.10
            fine = (value_noise(x * 0.45, y * 2.3, seed + 13) - 0.5) * 0.05
            v = shade + streak + grain + fine
            img.put(x, y, (BASE[0] * v, BASE[1] * v, BASE[2] * v))
    # Sparse long scratches catching the light.
    for k in range(w // 60):
        y = int((value_noise(k * 3.1, 0.7, seed + 31)) * (h - 8)) + 4
        x0 = int(value_noise(k * 5.7, 3.3, seed + 37) * w * 0.7)
        ln = 30 + int(value_noise(k * 1.9, 9.1, seed + 41) * 90)
        for x in range(x0, min(x0 + ln, w)):
            img.add(x, y, (70, 80, 95), 0.25)
            img.blend(x, y + 1, (10, 12, 18), 0.2)


def inset_plate(img, x0, y0, x1, y1, drop=0.86):
    """A machined inset: darker interior with a bevel (shadowed top/left lip,
    lit bottom/right edge - the surface drops INTO the panel)."""
    for y in range(y0, y1):
        for x in range(x0, x1):
            img.mul(x, y, drop)
    for x in range(x0, x1):
        img.blend(x, y0, (8, 10, 16), 0.75)
        img.blend(x, y0 + 1, (14, 18, 26), 0.4)
        img.add(x, y1 - 1, (90, 105, 125), 0.45)
    for y in range(y0, y1):
        img.blend(x0, y, (8, 10, 16), 0.6)
        img.add(x1 - 1, y, (70, 82, 100), 0.35)


def chamfer(img, corner, size):
    """A 45-degree machined cut on a top corner: dark facet + bright edge."""
    w = img.w
    for k in range(size):
        for t in range(size - k):
            x = (k) if corner == "l" else (w - 1 - k)
            img.blend(x, t, (10, 13, 20), 0.85)
        x = (size - k) if corner == "l" else (w - 1 - (size - k))
        img.add(x, k, (150, 170, 195), 0.5)
        img.add(x - (1 if corner == "l" else -1), k, (60, 70, 85), 0.3)


def glow_strip(img, x0, x1, y, strength=1.0):
    """A cyan light conduit with additive bloom above and below."""
    for x in range(x0, x1):
        flick = 0.85 + 0.15 * value_noise(x * 0.07, 1.0, 5)
        img.put(x, y, (200, 245, 255))
        for d in range(1, 5):
            a = strength * flick * (0.5 / (d * d))
            img.add(x, y - d, GLOW, a)
            img.add(x, y + d, GLOW, a)
        img.add(x, y, GLOW, 0.4)


def vent(img, x0, y0, n, vw=26, vh=4, gap=7):
    """A grille of dark slots with a lit lower lip."""
    for k in range(n):
        y = y0 + k * gap
        for x in range(x0, x0 + vw):
            for yy in range(y, y + vh):
                img.blend(x, yy, (4, 6, 10), 0.9)
            img.add(x, y + vh, (95, 110, 130), 0.5)
            img.blend(x, y - 1, (10, 13, 20), 0.5)


def led(img, cx, cy, col):
    for dy in range(-3, 4):
        for dx in range(-3, 4):
            d = math.hypot(dx, dy)
            if d < 1.2:
                img.put(cx + dx, cy + dy, (col[0] * 1.4, col[1] * 1.4, col[2] * 1.4))
            elif d < 3.2:
                img.add(cx + dx, cy + dy, col, 0.5 / (d * d))


def rivet(img, cx, cy):
    for dy in range(-2, 3):
        for dx in range(-2, 3):
            d = math.hypot(dx, dy)
            if d > 2.3:
                continue
            lit = 0.55 - (dx + dy) * 0.2 - d * 0.12
            img.blend(cx + dx, cy + dy, (200, 215, 235), max(0.2, min(0.9, lit)))
    img.add(cx - 1, cy - 1, (255, 255, 255), 0.6)


def tech_etch(img, x0, y0, x1, y1, seed):
    """Faint etched circuit traces: right-angle lines with node dots."""
    rngy = y1 - y0 - 6
    for k in range(max(2, (x1 - x0) // 90)):
        x = x0 + 8 + int(value_noise(k * 7.7, 2.2, seed) * (x1 - x0 - 40))
        y = y0 + 4 + int(value_noise(k * 3.3, 8.8, seed) * rngy)
        ln = 18 + int(value_noise(k * 9.1, 5.5, seed) * 30)
        for t in range(ln):
            img.add(min(x + t, x1 - 1), y, (60, 90, 110), 0.35)
        for t in range(8):
            img.add(min(x + ln, x1 - 1), min(y + t, y1 - 1), (60, 90, 110), 0.35)
        led(img, x, y, (40, 120, 140))


def edge_trim(img):
    """Bright machined lip + glow conduit along the top, dark base below."""
    w, h = img.w, img.h
    for x in range(w):
        img.put(x, 0, (210, 225, 245))
        img.blend(x, 1, (120, 140, 165), 0.8)
    glow_strip(img, 0, w, 3)
    for x in range(w):
        img.blend(x, h - 2, (8, 10, 16), 0.7)
        img.put(x, h - 1, (3, 5, 9))


def wing(w, h, seed):
    img = Img(w, h)
    metal_base(img, seed)
    # One big inset bay (the minimap / card sits visually inside it).
    inset_plate(img, 8, 14, w - 8, h - 12)
    edge_trim(img)
    chamfer(img, "l", 18)
    chamfer(img, "r", 18)
    for cx in (16, w - 17):
        rivet(img, cx, h - 8)
        rivet(img, cx, 10)
    led(img, w - 30, h - 8, (60, 220, 120))
    led(img, w - 44, h - 8, (230, 170, 50))
    return img


def console(w, h, seed):
    img = Img(w, h)
    metal_base(img, seed)
    # Machined plate bays along the strip, alternating widths.
    x, k = 14, 0
    while x < w - 120:
        pw = 150 if k % 2 == 0 else 96
        inset_plate(img, x, 16, min(x + pw, w - 14), h - 14)
        tech_etch(img, x + 4, 20, min(x + pw, w - 14) - 4, h - 24, seed + k)
        x += pw + 14
        k += 1
    inset_plate(img, x, 16, w - 14, h - 14)
    vent(img, w - 60, h - 44, 3)
    vent(img, 24, h - 44, 3)
    edge_trim(img)
    rivets = list(range(7, w - 6, 86))
    for cx in rivets:
        rivet(img, cx, 9)
        rivet(img, cx, h - 7)
    for i, col in enumerate([(60, 220, 120), (60, 220, 120), (230, 170, 50)]):
        led(img, 70 + i * 14, h - 36, col)
    return img


def main():
    os.makedirs(OUT, exist_ok=True)
    jobs = [
        ("wing_left.png", 184, 184, 11, wing),
        ("wing_right.png", 308, 184, 23, wing),
        ("console_mid.png", 1024, 124, 37, console),
    ]
    for name, w, h, seed, fn in jobs:
        img = fn(w, h, seed)
        write_png(os.path.join(OUT, name), w, h, img.px)
        print(f"  {name} {w}x{h}")


if __name__ == "__main__":
    main()
