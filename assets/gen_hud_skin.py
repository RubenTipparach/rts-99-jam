#!/usr/bin/env python3
"""Generate the in-game console skins (assets/hud/*.png), pure stdlib.

Two themed sets of three panels for the inverted-T HUD (see
apps/client/src/hud.rs): a left wing (the minimap housing), a right wing
(the command card), and a wide centre console strip stretched between them.
The HUD picks the set matching the player's faction:

- **hollow_*** - the Hollowmen flight deck: matte dark blue-grey steel,
  slotted bolts, rivets, indicator LEDs, instrument buttons, sagging cables.
- **astro_*** - the Astromancer lectern: muted porcelain grown-shell, auric
  gold filigree hairlines, crystal cabochons instead of bolts, gold/cyan
  rune lights, and an aether conduit under the lip (see
  docs/factions/astromancers.md for the language).

Both keep SHARP square corners and dark display wells behind the content.

Run: python3 assets/gen_hud_skin.py
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "worldgen"))
from common import write_png, value_noise, clamp8  # noqa: E402

OUT = os.path.join(os.path.dirname(__file__), "hud")

THEMES = {
    "hollow": dict(
        base=(66, 74, 90),       # dark blue-grey steel
        screen=(3, 9, 14),
        screen_tint=(0, 14, 10),  # faint green phosphor
        lip=(130, 145, 168),
        lip2=(84, 96, 116),
        glow=(70, 200, 245),
        conduit=(110, 190, 215),
        detail="industrial",
    ),
    "astro": dict(
        base=(104, 98, 90),      # muted porcelain bone
        screen=(5, 6, 16),
        screen_tint=(10, 8, 30),  # deep indigo glass
        lip=(212, 184, 126),     # auric gold
        lip2=(150, 128, 86),
        glow=(140, 230, 255),    # aether
        conduit=(170, 215, 240),
        detail="arcane",
    ),
}
T = THEMES["hollow"]  # active theme; main() swaps it per set

GOLD = (212, 184, 126)


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


def shell_base(img, seed):
    """The frame material: matte, gently top-lit. Steel gets horizontal
    brushed grain; porcelain gets a softer, almost grainless surface with a
    faint warm mottle (a grown shell, not a milled plate)."""
    w, h = img.w, img.h
    arcane = T["detail"] == "arcane"
    for y in range(h):
        t = y / max(h - 1, 1)
        shade = 0.92 - 0.30 * t
        shade += 0.18 * math.exp(-(((t - 0.14) / 0.10) ** 2))
        streak = (value_noise(0.0, y * 1.7, seed) - 0.5) * (0.05 if arcane else 0.12)
        for x in range(w):
            if arcane:
                mottle = (value_noise(x * 0.03, y * 0.05, seed + 7) - 0.5) * 0.08
                v = shade + streak + mottle
            else:
                grain = (value_noise(x * 0.05, y * 1.7, seed + 7) - 0.5) * 0.08
                fine = (value_noise(x * 0.45, y * 2.3, seed + 13) - 0.5) * 0.04
                v = shade + streak + grain + fine
            b = T["base"]
            img.put(x, y, (b[0] * v, b[1] * v, b[2] * v))
    if not arcane:
        for k in range(w // 60):  # sparse scratches on the metal only
            y = int(value_noise(k * 3.1, 0.7, seed + 31) * (h - 8)) + 4
            x0 = int(value_noise(k * 5.7, 3.3, seed + 37) * w * 0.7)
            ln = 30 + int(value_noise(k * 1.9, 9.1, seed + 41) * 90)
            for x in range(x0, min(x0 + ln, w)):
                img.add(x, y, (45, 52, 62), 0.18)
                img.blend(x, y + 1, (10, 12, 18), 0.15)


def screen(img, x0, y0, x1, y1):
    """A dark display well behind the section's content: a crisp bezel
    stepping down into near-black glass with faint scanlines and a sheen."""
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
    s, tint = T["screen"], T["screen_tint"]
    for y in range(y0, y1):
        t = (y - y0) / max(y1 - y0 - 1, 1)
        for x in range(x0, x1):
            c = (
                s[0] + tint[0] * 0.20,
                s[1] + tint[1] * 0.35 * (1.0 - t * 0.5),
                s[2] + tint[2] * 0.35,
            )
            img.put(x, y, c)
            if (y - y0) % 3 == 0:
                img.blend(x, y, (0, 0, 0), 0.30)
    for k in range(x0, x1):  # faint diagonal sheen
        y = y0 + int((k - x0) * 0.22)
        for d in range(10):
            img.add(k, y + d, (40, 60, 70), 0.05 * (1.0 - d / 10.0))


def bolt(img, cx, cy):
    """A slotted corner bolt (industrial hardware)."""
    for dy in range(-3, 4):
        for dx in range(-3, 4):
            d = math.hypot(dx, dy)
            if d > 3.4:
                continue
            lit = 0.6 - (dx + dy) * 0.16 - d * 0.09
            img.blend(cx + dx, cy + dy, (190, 205, 228), max(0.25, min(0.95, lit)))
    for t in range(-2, 3):
        img.blend(cx + t, cy + t, (12, 16, 24), 0.85)
    img.add(cx - 1, cy - 2, (255, 255, 255), 0.7)


def gem(img, cx, cy):
    """A crystal cabochon in a gold collet (arcane hardware): the
    Astromancer answer to a corner bolt."""
    for dy in range(-3, 4):
        for dx in range(-3, 4):
            d = math.hypot(dx, dy)
            if d > 3.6 or d <= 2.2:
                continue
            img.blend(cx + dx, cy + dy, GOLD, 0.8)
    for dy in range(-2, 3):
        for dx in range(-2, 3):
            d = math.hypot(dx, dy)
            if d > 2.2:
                continue
            depth = 1.0 - d / 2.6
            img.put(cx + dx, cy + dy, (60 + 100 * depth, 160 + 60 * depth, 220 + 30 * depth))
    img.add(cx - 1, cy - 1, (255, 255, 255), 0.85)
    for dy in range(-4, 5):
        for dx in range(-4, 5):
            d = math.hypot(dx, dy)
            if 2.5 < d < 4.5:
                img.add(cx + dx, cy + dy, T["glow"], 0.10)


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
    """Dotted indicator lights. Industrial mixes four colors with a few
    dead; arcane runs a gold/aether rune line, all lit."""
    if T["detail"] == "arcane":
        cols = [GOLD, T["glow"]]
        for k in range(n):
            led(img, x0 + k * pitch, y, cols[k % 2], 0.9)
        return
    cols = [(60, 220, 120), (70, 200, 245), (230, 170, 50), (220, 70, 60)]
    for k in range(n):
        r = value_noise(k * 4.7, 1.3, seed)
        led(img, x0 + k * pitch, y, cols[int(r * 17) % 4], 1.0 if r > 0.35 else 0.0)


def button(img, x, y, w=10, h=7, glow=None):
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
    """A cable sagging between two clips (industrial frames only)."""
    span = max(x1 - x0, 1)
    for x in range(x0, x1):
        t = (x - x0) / span
        y = y_top + sag * 4.0 * t * (1.0 - t)
        wob = (value_noise(x * 0.2, 0.5, seed) - 0.5) * 1.2
        yy = int(y + wob)
        img.put(x, yy + 1, (10, 12, 17))
        img.put(x, yy, (28, 32, 40))
        img.add(x, yy - 1, (90, 100, 115), 0.25)
    for cx in (x0, x1 - 1):
        for yy in range(y_top - 2, y_top + 2):
            img.put(cx, yy, (150, 165, 190))
            img.put(cx + 1, yy, (60, 70, 85))


def filigree(img, x0, x1, y):
    """Auric hairlines with corner curls: the Astromancer 'wiring'. A double
    gold line with small stepped volutes at the ends."""
    for x in range(x0, x1):
        img.add(x, y, GOLD, 0.55)
        img.add(x, y + 2, GOLD, 0.3)
    for (ex, sgn) in ((x0, 1), (x1 - 1, -1)):
        for k in range(4):  # a tiny stepped curl
            img.add(ex + sgn * k, y - 1 - k // 2, GOLD, 0.55)
            img.add(ex + sgn * (k + 1), y - 1 - k // 2, GOLD, 0.4)
        led(img, ex + sgn * 6, y + 1, T["glow"], 0.8)


def edge_trim(img):
    """Lip + glow conduit along the top, dark base below. SHARP corners."""
    w, h = img.w, img.h
    for x in range(w):
        img.put(x, 0, T["lip"])
        img.blend(x, 1, T["lip2"], 0.7)
    for x in range(w):
        flick = 0.85 + 0.15 * value_noise(x * 0.07, 1.0, 5)
        img.blend(x, 3, T["conduit"], 0.8)
        for d in range(1, 3):
            img.add(x, 3 - d, T["glow"], flick * 0.22 / (d * d))
            img.add(x, 3 + d, T["glow"], flick * 0.22 / (d * d))
    for x in range(w):
        img.blend(x, h - 2, (8, 10, 16), 0.7)
        img.put(x, h - 1, (3, 5, 9))
    for y in range(h):
        img.add(0, y, T["lip2"], 0.5)
        img.blend(w - 1, y, (6, 8, 13), 0.6)


def corner(img, cx, cy):
    if T["detail"] == "arcane":
        gem(img, cx, cy)
    else:
        bolt(img, cx, cy)


def wing(w, h, seed):
    img = Img(w, h)
    shell_base(img, seed)
    screen(img, 10, 12, w - 10, h - 16)
    edge_trim(img)
    for cx, cy in [(6, 8), (w - 7, 8), (6, h - 8), (w - 7, h - 8)]:
        corner(img, cx, cy)
    led_strip(img, 22, h - 8, (w - 44) // 9, seed)
    return img


def console(w, h, seed):
    img = Img(w, h)
    shell_base(img, seed)
    screen(img, 12, 14, w - 12, h - 22)
    edge_trim(img)
    for cx, cy in [(6, 8), (w - 7, 8), (6, h - 7), (w - 7, h - 7)]:
        corner(img, cx, cy)
    if T["detail"] == "arcane":
        # The bottom rail is an illuminated border: filigree runs with rune
        # lights, and a gem keystone at the centre. No cables, no rivets -
        # the shell is grown in one piece.
        filigree(img, 30, w // 2 - 40, h - 8)
        filigree(img, w // 2 + 40, w - 30, h - 8)
        led_strip(img, w // 2 - 28, h - 6, 7, seed, pitch=9)
        gem(img, w // 2, h - 8)
    else:
        for cx in range(40, w - 40, 120):
            rivet(img, cx, h - 5)
        led_strip(img, 60, h - 5, 8, seed + 3)
        led_strip(img, w - 150, h - 5, 8, seed + 9)
        bx = w // 2 - 30
        button(img, bx, h - 9)
        button(img, bx + 14, h - 9)
        button(img, bx + 28, h - 9, glow=T["glow"])
        button(img, bx + 42, h - 9)
        wire(img, w - 320, w - 180, h - 6, 3.0, seed)
        wire(img, 150, 260, h - 6, 2.2, seed + 4)
    return img


def main():
    global T
    os.makedirs(OUT, exist_ok=True)
    jobs = [
        ("wing_left.png", 184, 184, 11, wing),
        ("wing_right.png", 308, 184, 23, wing),
        ("console_mid.png", 1024, 124, 37, console),
    ]
    for theme in ("hollow", "astro"):
        T = THEMES[theme]
        for name, w, h, seed, fn in jobs:
            img = fn(w, h, seed)
            write_png(os.path.join(OUT, f"{theme}_{name}"), w, h, img.px)
            print(f"  {theme}_{name} {w}x{h}")


if __name__ == "__main__":
    main()
