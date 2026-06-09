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


def diamond(cv, cx, cy, a, b, swatch, kind="rock"):
    """Iso (diamond) world thumbnail, matching menu.rs: surface colour + its
    defining landform (craters / volcanoes) + blue/red start positions."""
    cx, cy, a, b = int(cx), int(cy), int(a), int(b)

    def sc(m):
        return tuple(min(255, max(0, int(c * m))) for c in swatch)

    for yy in range(cy - b, cy + b + 1):
        for xx in range(cx - a, cx + a + 1):
            f = abs((xx - cx) / a) + abs((yy - cy) / b)
            if f <= 1.0:
                cv.set(xx, yy, swatch if f < 0.93 else (120, 160, 210))

    clip = (cx - a, cy - b, cx + a + 1, cy + b + 1)

    def iso(u, v):
        return (cx + (u - v) * a, cy + (u + v - 1) * b)

    if kind == "io":
        for u, v in [(0.42, 0.46), (0.62, 0.6), (0.32, 0.68)]:
            px, py = iso(u, v)
            disc(cv, px, py, 16 * SS, 10 * SS, sc(1.15), clip)
            disc(cv, px, py, 6 * SS, 4 * SS, (236, 122, 44), clip)
    else:
        for u, v in [(0.3, 0.3), (0.55, 0.45), (0.7, 0.6), (0.4, 0.72), (0.62, 0.26)]:
            px, py = iso(u, v)
            disc(cv, px, py, 9 * SS, 6 * SS, sc(1.25), clip)
            disc(cv, px, py, 5 * SS, 3 * SS, sc(0.7), clip)
    px, py = iso(0.3, 0.3)
    disc(cv, px, py, 5 * SS, 4 * SS, (74, 163, 255), clip)
    px, py = iso(0.7, 0.7)
    disc(cv, px, py, 5 * SS, 4 * SS, (255, 90, 74), clip)


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
    text(cv, rx, oy + 236 * SS, "MAP:  IO   (8/22)", 2 * SS, (220, 230, 246))
    diamond(cv, rx + 180 * SS, oy + 348 * SS, 168 * SS, 78 * SS, (210, 190, 90), "io")
    button(cv, rx, oy + 436 * SS, 360 * SS, 44 * SS, "SELECT MAP", NORMAL)

    button(cv, ox + 30 * SS, oy + h - 86 * SS, 200 * SS, 56 * SS, "BACK", NORMAL)
    button(cv, ox + w - 230 * SS, oy + h - 86 * SS, 200 * SS, 56 * SS, "START", SELECTED)


def modal(cv, ox, oy, w, h):
    """The map-select modal: scrollable world list + scrollbar + live preview."""
    text(cv, ox + 16 * SS, oy + 12 * SS, "MAP SELECT MODAL  (SCROLLABLE)", 2 * SS, (110, 140, 180))
    cv.fill_rect(ox, oy + 26 * SS, ox + w, oy + h, (6, 9, 16))
    mw, mh = int(w * 0.84), int(h * 0.84)
    mx, my = ox + (w - mw) // 2, oy + 26 * SS + (h - 26 * SS - mh) // 2
    border_rect(cv, mx, my, mw, mh, (12, 18, 32), (120, 160, 210))
    text(cv, mx + 24 * SS, my + 24 * SS, "SELECT BATTLEFIELD", 3 * SS, (231, 238, 250))

    rows = [
        ("LUNA", (150, 148, 142)), ("CERES", (78, 76, 76)), ("VESTA", (150, 140, 120)),
        ("MARS", (170, 96, 60)), ("CALLISTO", (120, 112, 104)), ("GANYMEDE", (150, 156, 168)),
        ("EUROPA", (220, 210, 196)), ("IO", (210, 190, 90)), ("TITAN", (180, 120, 60)),
    ]
    lx, ly, lw, rh = mx + 24 * SS, my + 70 * SS, 300 * SS, 40 * SS
    for i, (nm, sw_) in enumerate(rows):
        st = SELECTED if nm == "IO" else NORMAL
        button(cv, lx, ly + i * rh, lw, rh - 6 * SS, nm, st)
        cv.fill_rect(lx + 8 * SS, ly + i * rh + 7 * SS, lx + 26 * SS, ly + i * rh + rh - 9 * SS, sw_)
    # scrollbar (more worlds below: 22 total)
    sbx = lx + lw + 8 * SS
    button(cv, sbx, ly, 26 * SS, 30 * SS, "^", NORMAL)
    button(cv, sbx, ly + 9 * rh - 30 * SS, 26 * SS, 30 * SS, "v", NORMAL)
    cv.fill_rect(sbx, ly + 32 * SS, sbx + 26 * SS, ly + 9 * rh - 32 * SS, (40, 52, 74))
    cv.fill_rect(sbx, ly + 32 * SS, sbx + 26 * SS, ly + 32 * SS + 130 * SS, (130, 170, 220))
    # live preview of the highlighted world
    px = lx + lw + 56 * SS
    pcx = (px + mx + mw - 24 * SS) // 2
    text(cv, pcx - text_w("IO", 2 * SS) // 2, my + 52 * SS, "IO", 2 * SS, (207, 224, 245))
    pa = (mx + mw - 24 * SS - px) // 2 - 12 * SS
    diamond(cv, pcx, my + mh // 2 + 10 * SS, pa, int(pa * 0.6), (210, 190, 90), "io")
    button(cv, mx + mw - 180 * SS, my + mh - 62 * SS, 150 * SS, 44 * SS, "DONE", SELECTED)


def main():
    os.makedirs(OUT_DIR, exist_ok=True)
    sw, sh = 1100 * SS, 560 * SS
    margin = 24 * SS
    gap = 24 * SS
    W = margin * 2 + sw
    H = margin * 2 + 3 * sh + 2 * gap + 28 * SS
    cv = Canvas(W, H)
    cv.fill_rect(0, 0, W, H, (10, 12, 18))
    text(cv, margin, margin, "FRONT-END MOCKUP  (APPROX OF menu.rs)", 2 * SS, (140, 160, 195))
    y0 = margin + 28 * SS
    main_menu(cv, margin, y0, sw, sh)
    lobby(cv, margin, y0 + sh + gap, sw, sh)
    modal(cv, margin, y0 + 2 * (sh + gap), sw, sh)
    fw, fh, out = downsample(cv)
    path = os.path.join(OUT_DIR, "menu.png")
    write_png(path, fw, fh, out)
    print("wrote", path, fw, "x", fh)


if __name__ == "__main__":
    main()
