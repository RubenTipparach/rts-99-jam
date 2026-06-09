#!/usr/bin/env python3
"""Mockup of the front-end screens (main menu + skirmish lobby) for review.

Approximates the layout drawn by apps/client/src/menu.rs (the real screens render
on the HUD canvas at runtime). Pixel font, so it reads chunky; the live build
uses the browser's monospace.

Run:  python3 assets/render_menu_mock.py
Out:  assets/concepts/menu.png
"""

import os
from concept_kit import SS, Canvas, downsample, write_png, text, text_w

OUT_DIR = os.path.join(os.path.dirname(__file__), "concepts")

NORMAL = ((18, 26, 44), (120, 160, 210), (220, 230, 246))
SELECTED = ((40, 86, 150), (150, 200, 255), (234, 242, 255))
DISABLED = ((28, 34, 48), (70, 84, 110), (94, 106, 130))


def border_rect(cv, x, y, w, h, fill, border, t=2 * SS):
    cv.fill_rect(x, y, x + w, y + h, fill)
    cv.fill_rect(x, y, x + w, y + t, border)
    cv.fill_rect(x, y + h - t, x + w, y + h, border)
    cv.fill_rect(x, y, x + t, y + h, border)
    cv.fill_rect(x + w - t, y, x + w, y + h, border)


def button(cv, x, y, w, h, label, state, scale=2 * SS):
    fill, border, txt = state
    border_rect(cv, x, y, w, h, fill, border)
    lw = text_w(label, scale)
    text(cv, x + (w - lw) // 2, y + (h - 7 * scale) // 2, label, scale, txt)


def disc(cv, cx, cy, rx, ry, col, clip):
    cx0, cy0, cx1, cy1 = clip
    for yy in range(max(int(cy - ry), cy0), min(int(cy + ry) + 1, cy1)):
        for xx in range(max(int(cx - rx), cx0), min(int(cx + rx) + 1, cx1)):
            if ((xx - cx) / rx) ** 2 + ((yy - cy) / ry) ** 2 <= 1.0:
                cv.set(xx, yy, col)


def map_preview(cv, x, y, w, h, mp):
    border_rect(cv, x, y, w, h, (10, 20, 38), (120, 160, 210))
    clip = (int(x + 2 * SS), int(y + 2 * SS), int(x + w - 2 * SS), int(y + h - 2 * SS))
    if mp == 0:
        disc(cv, x + w * 0.5, y + h * 0.5, w * 0.30, h * 0.42, (42, 58, 48), clip)
        disc(cv, x + w * 0.5, y + h * 0.5, w * 0.19, h * 0.27, (51, 70, 58), clip)
        disc(cv, x + w * 0.30, y + h * 0.30, 6 * SS, 6 * SS, (74, 163, 255), clip)
        disc(cv, x + w * 0.70, y + h * 0.70, 6 * SS, 6 * SS, (255, 90, 74), clip)
    else:
        disc(cv, x + w * 0.28, y + h * 0.5, w * 0.20, h * 0.34, (47, 63, 76), clip)
        disc(cv, x + w * 0.72, y + h * 0.5, w * 0.20, h * 0.34, (47, 63, 76), clip)
        disc(cv, x + w * 0.22, y + h * 0.5, 6 * SS, 6 * SS, (74, 163, 255), clip)
        disc(cv, x + w * 0.78, y + h * 0.5, 6 * SS, 6 * SS, (255, 90, 74), clip)


def panel_bg(cv, x0, y0, x1, y1):
    h = y1 - y0
    for y in range(y0, y1):
        t = (y - y0) / h
        cv.fill_rect(x0, y, x1, y + 1,
                     (int(8 + 6 * (1 - t)), int(12 + 10 * (1 - t)), int(22 + 16 * (1 - t))))
    # dim layer over the "3D scene"
    for y in range(y0, y1):
        cv.fill_rect(x0, y, x1, y + 1, (6, 9, 16)) if False else None


def main_menu(cv, ox, oy, w, h):
    panel_bg(cv, ox, oy, ox + w, oy + h)
    text(cv, ox + 16 * SS, oy + 12 * SS, "MAIN MENU", 2 * SS, (110, 140, 180))
    title = "ASTROMANCERS"
    text(cv, ox + (w - text_w(title, 6 * SS)) // 2, oy + int(h * 0.22), title, 6 * SS, (231, 238, 250))
    sub = "A DETERMINISTIC LOCKSTEP RTS"
    text(cv, ox + (w - text_w(sub, 2 * SS)) // 2, oy + int(h * 0.22) + 56 * SS, sub, 2 * SS, (127, 158, 200))
    bw = 280 * SS
    bx = ox + (w - bw) // 2
    button(cv, bx, oy + int(h * 0.50), bw, 56 * SS, "CAMPAIGN (SOON)", DISABLED)
    button(cv, bx, oy + int(h * 0.50) + 72 * SS, bw, 56 * SS, "SKIRMISH", NORMAL)


def lobby(cv, ox, oy, w, h):
    panel_bg(cv, ox, oy, ox + w, oy + h)
    text(cv, ox + 30 * SS, oy + 26 * SS, "SKIRMISH - LOBBY", 4 * SS, (231, 238, 250))
    text(cv, ox + 30 * SS, oy + 84 * SS, "CHOOSE YOUR FACTION", 2 * SS, (154, 178, 216))
    facs = [
        ("ASTROMANCERS", "ok"), ("HOLLOWMEN", "sel"), ("NINEFOLD", "no"),
        ("WARREN", "no"), ("RIMELINGS", "no"),
    ]
    for i, (name, st) in enumerate(facs):
        state = SELECTED if st == "sel" else (NORMAL if st == "ok" else DISABLED)
        button(cv, ox + 30 * SS, oy + 110 * SS + i * 60 * SS, 320 * SS, 48 * SS, name, state)

    rx = ox + w - 400 * SS
    text(cv, rx, oy + 84 * SS, "PLAYERS", 2 * SS, (154, 178, 216))
    text(cv, rx, oy + 116 * SS, "P1  YOU   -  HOLLOWMEN", 2 * SS, (220, 230, 246))
    text(cv, rx, oy + 142 * SS, "P2  BOT   -  ASTROMANCERS", 2 * SS, (220, 230, 246))
    text(cv, rx + 70 * SS, oy + 196 * SS, "BOTS: 1", 2 * SS, (154, 178, 216))
    button(cv, rx, oy + 184 * SS, 56 * SS, 44 * SS, "-", DISABLED)
    button(cv, rx + 320 * SS, oy + 184 * SS, 56 * SS, 44 * SS, "+", NORMAL)
    text(cv, rx, oy + 250 * SS, "MAP:  RUINS OF AETHER", 2 * SS, (220, 230, 246))
    map_preview(cv, rx, oy + 264 * SS, 360 * SS, 150 * SS, 0)
    button(cv, rx, oy + 264 * SS + 162 * SS, 360 * SS, 44 * SS, "NEXT MAP", NORMAL)

    button(cv, ox + 30 * SS, oy + h - 86 * SS, 200 * SS, 56 * SS, "BACK", NORMAL)
    button(cv, ox + w - 230 * SS, oy + h - 86 * SS, 200 * SS, 56 * SS, "START", SELECTED)


def main():
    os.makedirs(OUT_DIR, exist_ok=True)
    sw, sh = 1100 * SS, 560 * SS
    margin = 24 * SS
    gap = 24 * SS
    W = margin * 2 + sw
    H = margin * 2 + 2 * sh + gap + 28 * SS
    cv = Canvas(W, H)
    cv.fill_rect(0, 0, W, H, (10, 12, 18))
    text(cv, margin, margin, "FRONT-END MOCKUP  (APPROX OF menu.rs)", 2 * SS, (140, 160, 195))
    y0 = margin + 28 * SS
    main_menu(cv, margin, y0, sw, sh)
    lobby(cv, margin, y0 + sh + gap, sw, sh)
    fw, fh, out = downsample(cv)
    path = os.path.join(OUT_DIR, "menu.png")
    write_png(path, fw, fh, out)
    print("wrote", path, fw, "x", fh)


if __name__ == "__main__":
    main()
