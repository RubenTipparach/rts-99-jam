#!/usr/bin/env python3
"""Generate the in-game console skin (assets/hud/*.png), pure stdlib.

Three panels for the inverted-T HUD (see apps/client/src/hud.rs): a left
wing (the minimap housing), a right wing (the command card), and a wide
centre console strip stretched between them. The design language is a
flight-deck instrument: a machined steel frame with SHARP corners around a
dark display well (the "screen" the minimap / card / selection render on),
dressed with bolts, rivets, dotted indicator lights, little buttons, and a
cable sagging along the frame.

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
SCREEN = (3, 9, 14)  # the dark display well (deep blue-green black)


class Img:
    def __init__(self, w, h):
        self.w, self.h = w, h
        self.px = bytearray(w * h * 3)

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
        if a <= 0.0 or not (0 <= x < self.w and 0 <= y < self.h):
            return
        i = (y * self.w + x) * 3
        self.px[i] = clamp8(self.px[i] + c[0] * a)
        self.px[i + 1] = clamp8(self.px[i + 1] + c[1] * a)
        self.px[i + 2] = clamp8(self.px[i + 2] + c[2] * a)


def metal_base(img, seed):
    """Steel with a specular reflection band and brushed horizontal grain."""
    w, h = img.w, img.h
    for y in range(h):
        t = y / max(h - 1, 1)
        shade = 0.80 - 0.40 * t
        shade += 0.55 * math.exp(-(((t - 0.14) / 0.09) ** 2))  # spec band
        shade += 0.10 * math.exp(-(((t - 0.55) / 0.25) ** 2))
        streak = (value_noise(0.0, y * 1.7, seed) - 0.5) * 0.16
        for x in range(w):
            grain = (value_noise(x * 0.05, y * 1.7, seed + 7) - 0.5) * 0.10
            fine = (value_noise(x * 0.45, y * 2.3, seed + 13) - 0.5) * 0.05
            v = shade + streak + grain + fine
            img.put(x, y, (BASE[0] * v, BASE[1] * v, BASE[2] * v))
    for k in range(w // 60):  # sparse scratches catching the light
        y = int(value_noise(k * 3.1, 0.7, seed + 31) * (h - 8)) + 4
        x0 = int(value_noise(k * 5.7, 3.3, seed + 37) * w * 0.7)
        ln = 30 + int(value_noise(k * 1.9, 9.1, seed + 41) * 90)
        for x in range(x0, min(x0 + ln, w)):
            img.add(x, y, (70, 80, 95), 0.25)
            img.blend(x, y + 1, (10, 12, 18), 0.2)


def screen(img, x0, y0, x1, y1, seed, tint=(0, 14, 10)):
    """A dark display well behind the section's content: a crisp monitor
    bezel (outer ridge highlight, inner shadow), a deep blue-green black
    face with faint scanlines, and a soft diagonal sheen."""
    # bezel: raised outer ridge, then a shadow stepping down into the glass
    for x in range(x0 - 2, x1 + 2):
        img.add(x, y0 - 2, (110, 125, 150), 0.5)
        img.blend(x, y1 + 1, (200, 215, 235), 0.25)
    for y in range(y0 - 2, y1 + 2):
        img.add(x0 - 2, y, (90, 105, 125), 0.4)
        img.blend(x1 + 1, y, (160, 175, 195), 0.25)
    for x in range(x0 - 1, x1 + 1):
        img.put(x, y0 - 1, (5, 7, 12))
        img.put(x, y1, (16, 22, 32))
    for y in range(y0 - 1, y1 + 1):
        img.put(x0 - 1, y, (8, 11, 17))
        img.put(x1, y, (12, 16, 24))
    # the glass: near-black with a whisper of phosphor color + scanlines
    for y in range(y0, y1):
        t = (y - y0) / max(y1 - y0 - 1, 1)
        for x in range(x0, x1):
            c = (
                SCREEN[0] + tint[0] * 0.20,
                SCREEN[1] + tint[1] * 0.35 * (1.0 - t * 0.5),
                SCREEN[2] + tint[2] * 0.35,
            )
            img.put(x, y, c)
            if (y - y0) % 3 == 0:
                img.blend(x, y, (0, 0, 0), 0.30)
    # diagonal glass sheen, very faint
    for k in range(x0, x1):
        y = y0 + int((k - x0) * 0.22)
        for d in range(10):
            img.add(k, y + d, (40, 60, 70), 0.05 * (1.0 - d / 10.0))


def bolt(img, cx, cy):
    """A hex-ish corner bolt with a slot and a catchlight."""
    for dy in range(-3, 4):
        for dx in range(-3, 4):
            d = math.hypot(dx, dy)
            if d > 3.4:
                continue
            lit = 0.6 - (dx + dy) * 0.16 - d * 0.09
            img.blend(cx + dx, cy + dy, (190, 205, 228), max(0.25, min(0.95, lit)))
    for t in range(-2, 3):  # the screwdriver slot
        img.blend(cx + t, cy + t, (12, 16, 24), 0.85)
    img.add(cx - 1, cy - 2, (255, 255, 255), 0.7)


def rivet(img, cx, cy):
    for dy in range(-1, 2):
        for dx in range(-1, 2):
            d = math.hypot(dx, dy)
            lit = 0.6 - (dx + dy) * 0.22 - d * 0.1
            img.blend(cx + dx, cy + dy, (200, 215, 235), max(0.25, min(0.9, lit)))
    img.add(cx, cy - 1, (255, 255, 255), 0.5)


def led(img, cx, cy, col, lit=1.0):
    img.put(cx, cy, (col[0] * (0.4 + lit), col[1] * (0.4 + lit), col[2] * (0.4 + lit)))
    if lit > 0.4:
        for dy in range(-2, 3):
            for dx in range(-2, 3):
                d = math.hypot(dx, dy)
                if 0.5 < d < 2.6:
                    img.add(cx + dx, cy + dy, col, 0.35 / (d * d))


def led_strip(img, x0, y, n, seed, pitch=9):
    """Dotted indicator lights: mixed colors, a few dark (off)."""
    cols = [(60, 220, 120), (70, 200, 245), (230, 170, 50), (220, 70, 60)]
    for k in range(n):
        r = value_noise(k * 4.7, 1.3, seed)
        col = cols[int(r * 17) % 4]
        lit = 1.0 if r > 0.35 else 0.0
        led(img, x0 + k * pitch, y, col, lit)


def button(img, x, y, w=10, h=7, glow=None):
    """A little rectangular instrument button with a top highlight."""
    for yy in range(y, y + h):
        for xx in range(x, x + w):
            img.put(xx, yy, (40, 47, 60))
    for xx in range(x, x + w):
        img.add(xx, y, (140, 155, 180), 0.6)
        img.blend(xx, y + h - 1, (6, 8, 13), 0.8)
    if glow:
        for xx in range(x + 2, x + w - 2):
            for yy in range(y + 2, y + h - 2):
                img.put(xx, yy, glow)
                img.add(xx, yy - 1, glow, 0.2)


def wire(img, x0, x1, y_top, sag, seed):
    """A cable sagging between two clips on the frame."""
    span = max(x1 - x0, 1)
    for x in range(x0, x1):
        t = (x - x0) / span
        y = y_top + sag * 4.0 * t * (1.0 - t)
        wob = (value_noise(x * 0.2, 0.5, seed) - 0.5) * 1.2
        yy = int(y + wob)
        img.put(x, yy + 1, (10, 12, 17))
        img.put(x, yy, (28, 32, 40))
        img.add(x, yy - 1, (90, 100, 115), 0.25)
    for cx in (x0, x1 - 1):  # clips
        for yy in range(y_top - 2, y_top + 2):
            img.put(cx, yy, (150, 165, 190))
            img.put(cx + 1, yy, (60, 70, 85))


def edge_trim(img):
    """Bright machined lip + glow conduit along the top, dark base below.
    Corners stay SHARP: no chamfers, just crisp square edges."""
    w, h = img.w, img.h
    for x in range(w):
        img.put(x, 0, (210, 225, 245))
        img.blend(x, 1, (120, 140, 165), 0.8)
    for x in range(w):
        flick = 0.85 + 0.15 * value_noise(x * 0.07, 1.0, 5)
        img.put(x, 3, (200, 245, 255))
        for d in range(1, 4):
            img.add(x, 3 - d, GLOW, flick * 0.5 / (d * d))
            img.add(x, 3 + d, GLOW, flick * 0.5 / (d * d))
    for x in range(w):
        img.blend(x, h - 2, (8, 10, 16), 0.7)
        img.put(x, h - 1, (3, 5, 9))
    for y in range(h):  # crisp side edges
        img.add(0, y, (140, 155, 180), 0.4)
        img.blend(w - 1, y, (6, 8, 13), 0.6)


def wing(w, h, seed):
    img = Img(w, h)
    metal_base(img, seed)
    screen(img, 10, 12, w - 10, h - 16, seed)
    edge_trim(img)
    for cx, cy in [(6, 8), (w - 7, 8), (6, h - 8), (w - 7, h - 8)]:
        bolt(img, cx, cy)
    led_strip(img, 22, h - 8, (w - 44) // 9, seed)
    return img


def console(w, h, seed):
    img = Img(w, h)
    metal_base(img, seed)
    screen(img, 12, 14, w - 12, h - 22, seed)
    edge_trim(img)
    for cx, cy in [(6, 8), (w - 7, 8), (6, h - 7), (w - 7, h - 7)]:
        bolt(img, cx, cy)
    # the bottom frame rail: rivets, indicator lights, buttons, a cable
    for cx in range(40, w - 40, 120):
        rivet(img, cx, h - 5)
    led_strip(img, 60, h - 5, 8, seed + 3)
    led_strip(img, w - 150, h - 5, 8, seed + 9)
    bx = w // 2 - 30
    button(img, bx, h - 9)
    button(img, bx + 14, h - 9)
    button(img, bx + 28, h - 9, glow=(70, 200, 245))
    button(img, bx + 42, h - 9)
    wire(img, w - 320, w - 180, h - 6, 3.0, seed)
    wire(img, 150, 260, h - 6, 2.2, seed + 4)
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
