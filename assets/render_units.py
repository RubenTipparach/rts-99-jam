#!/usr/bin/env python3
"""Concept renderer for faction workers + resource nodes (no dependencies).

Draws the two launch factions' worker units in their idle/build/gather poses and
the ore-crystal and carbon-gas resource nodes, in the game's iso camera. Rough
massing + pose intent for animation, not final art.

Run:  python3 assets/render_units.py
Out:  assets/concepts/units_resources.png
"""

import os
import math

from concept_kit import (
    SS, Canvas, box, prism, frustum, pyramid, ring, render_object, downsample,
    write_png, gradient_bg, text, text_w,
)

OUT_DIR = os.path.join(os.path.dirname(__file__), "concepts")

# Astromancers (grown, hovering).
A_SHELL = (238, 233, 219)
A_SHELL2 = (214, 209, 192)
A_GOLD = (234, 204, 126)
A_VIO = (168, 92, 222)
A_COREV = (212, 150, 255)
A_TEAL = (96, 224, 210)
A_CORET = (168, 255, 246)
# Hollowmen (industrial, grounded).
H_STEEL = (124, 132, 142)
H_STEEL2 = (160, 168, 176)
H_DARK = (60, 66, 74)
H_HAZ = (214, 158, 54)
H_AMBER = (252, 200, 98)
H_CYAN = (112, 198, 226)
H_GUN = (46, 50, 56)
H_SKIN = (188, 150, 120)
# Resources.
ORE_BODY = (120, 214, 236)
ORE_CORE = (210, 248, 255)
ORE_DEEP = (70, 150, 196)
ROCK = (72, 82, 96)
ROCK_DK = (44, 52, 64)
GAS_GLOW = (130, 244, 156)
GAS_SMOKE = (120, 228, 140)
VENT = (54, 64, 60)
VENT_DK = (34, 42, 40)


def mote(mesh, x, y, z, s, color):
    box(mesh, (x - s, y - s, z - s), (x + s, y + s, z + s), color, emissive=True)


# ---------------------------------------------------------- Astromancer Acolyte
# A small hooded caster that hovers; "grows" buildings and draws motes of matter.
# Scale: ~2.2 tall. Hovers (no legs), trailing robe-point beneath.


def acolyte(pose):
    m = []
    yb = 0.85  # hovers above ground
    # trailing robe point (hangs in the air).
    frustum(m, 0, 0, 0.18, 0.62, yb - 0.55, yb + 0.05, A_SHELL2, n=6, top=False)
    # robe body, flaring out.
    frustum(m, 0, 0, 0.62, 0.46, yb + 0.05, yb + 1.0, A_SHELL, n=6)
    box(m, (-0.5, yb + 0.55, -0.12), (0.5, yb + 0.72, 0.12), A_GOLD)  # sash
    # shoulder mantle + hood.
    frustum(m, 0, 0, 0.5, 0.34, yb + 1.0, yb + 1.35, A_SHELL2, n=6)
    frustum(m, 0, 0, 0.34, 0.26, yb + 1.35, yb + 1.7, A_SHELL, n=6)
    pyramid(m, 0, 0, 0.3, yb + 1.55, yb + 2.0, A_SHELL2, n=6)  # hood peak
    box(m, (-0.16, yb + 1.42, 0.2), (0.16, yb + 1.56, 0.34), A_CORET, emissive=True)  # glowing eyes
    # focus orb + per-pose action.
    if pose == "IDLE":
        ox, oy, oz = 0.55, yb + 0.85, 0.35
        prism(m, ox, oz, 0.2, oy - 0.18, oy + 0.18, A_COREV, n=6, emissive=True)
    elif pose == "GROW":
        # orb raised, projecting a beam up into a sprouting crystal (a building grows).
        prism(m, 0, 0.42, 0.22, yb + 1.7, yb + 2.05, A_COREV, n=6, emissive=True)
        box(m, (-0.08, yb + 2.05, 0.34), (0.08, yb + 3.5, 0.5), A_TEAL, emissive=True)  # beam
        frustum(m, 0, 0.42, 0.0, 0.5, yb + 3.4, yb + 3.55, A_VIO, n=6)
        prism(m, 0, 0.42, 0.5, yb + 3.55, yb + 4.0, A_SHELL, n=6)
        pyramid(m, 0, 0.42, 0.5, yb + 4.0, yb + 4.8, A_GOLD, n=6)  # the structure forming
    elif pose == "GATHER":
        # orb low over a small ore shard, drawing glowing motes up into the robe.
        prism(m, 0.5, 0.5, 0.2, yb + 0.4, yb + 0.74, A_COREV, n=6, emissive=True)
        prism(m, 0.95, 0.9, 0.18, -0.0, 0.35, ORE_BODY, n=5)
        pyramid(m, 0.95, 0.9, 0.18, 0.35, 0.85, ORE_CORE, n=5)
        for i, t in enumerate((0.0, 0.35, 0.7)):
            mote(m, 0.95 - 0.4 * t, yb - 0.1 + 1.0 * t, 0.9 - 0.4 * t, 0.07, A_CORET)
    return m


# ---------------------------------------------------------- Hollowmen Engineer
# A stocky powered-armor worker that walks, welds, and drills. Scale: ~2.2 tall.


def engineer(pose):
    m = []
    # legs (posed).
    if pose == "WALK":
        lz = [(0.5, 0.9), (-0.7, -0.3)]  # one forward, one back (z ranges)
    else:
        lz = [(-0.32, 0.1), (-0.1, 0.32)]
    for i, (z0, z1) in enumerate(lz):
        sx = -0.34 if i == 0 else 0.06
        box(m, (sx, 0.0, z0), (sx + 0.28, 0.66, z1), H_DARK)
        box(m, (sx, 0.0, z0 - 0.02), (sx + 0.28, 0.12, z1 + 0.18), H_GUN)  # boot
    # torso + hazard chest + backpack.
    box(m, (-0.44, 0.66, -0.34), (0.44, 1.5, 0.34), H_STEEL)
    box(m, (-0.44, 1.0, -0.34), (0.44, 1.18, 0.36), H_HAZ)  # chest stripe
    box(m, (-0.34, 0.8, -0.56), (0.34, 1.46, -0.34), H_DARK)  # backpack
    box(m, (-0.24, 1.2, -0.58), (0.24, 1.4, -0.54), H_AMBER, emissive=True)  # pack vent
    # head + visor.
    box(m, (-0.26, 1.5, -0.24), (0.26, 1.98, 0.24), H_STEEL2)
    box(m, (-0.26, 1.66, 0.22), (0.26, 1.84, 0.3), H_CYAN, emissive=True)  # visor
    box(m, (-0.3, 1.96, -0.26), (0.3, 2.06, 0.26), H_DARK)  # crown
    # arms + per-pose tool.
    box(m, (-0.62, 0.8, -0.16), (-0.44, 1.46, 0.16), H_STEEL)  # left arm
    if pose == "WALK":
        box(m, (0.44, 0.8, -0.16), (0.62, 1.46, 0.16), H_STEEL)
        box(m, (0.46, 0.74, 0.0), (0.6, 1.0, 0.5), H_GUN)  # carried wrench
    elif pose == "BUILD":
        # welder arm raised to a girder, throwing sparks.
        box(m, (0.44, 1.0, 0.0), (0.62, 1.5, 0.2), H_STEEL)
        box(m, (0.5, 1.36, 0.2), (0.66, 1.52, 0.9), H_GUN)  # welder
        box(m, (0.9, 0.0, 0.5), (1.1, 1.7, 0.7), H_STEEL2)  # girder being welded
        box(m, (0.86, 0.0, 0.46), (1.14, 0.16, 0.74), H_HAZ)
        for sx, sy, sz in ((0.78, 1.5, 0.78), (0.66, 1.62, 0.66), (0.86, 1.34, 0.7)):
            mote(m, sx, sy, sz, 0.06, H_AMBER)
        mote(m, 0.74, 1.46, 0.74, 0.1, (255, 246, 210))
    elif pose == "GATHER":
        # drill arm down into an ore shard; a glowing ore canister on the back.
        box(m, (0.44, 0.7, 0.0), (0.62, 1.3, 0.2), H_STEEL)
        box(m, (0.52, 0.2, 0.2), (0.66, 0.9, 0.4), H_GUN)  # drill housing
        pyramid(m, 0.59, 0.3, 0.12, 0.2, -0.15, H_STEEL2, n=5)  # drill bit (down)
        prism(m, 0.95, 0.78, 0.22, 0.0, 0.5, ORE_BODY, n=5)  # ore shard at feet
        pyramid(m, 0.95, 0.78, 0.22, 0.5, 1.0, ORE_CORE, n=5)
        box(m, (-0.3, 0.9, -0.62), (0.3, 1.4, -0.5), ORE_DEEP)  # ore canister
        box(m, (-0.24, 1.0, -0.64), (0.24, 1.3, -0.6), ORE_CORE, emissive=True)
        for i, t in enumerate((0.0, 0.5)):
            mote(m, 0.7 + 0.1 * t, 0.3 + 0.2 * t, 0.4, 0.06, ORE_CORE)
    return m


# ------------------------------------------------------------- resource nodes


def ore_node(level):
    # A cluster of shiny, translucent crystals erupting from a dark rock base.
    # "Shine" is faked: emissive cores + near-white tips + bright facet edges.
    m = []
    frustum(m, 0, 0, 1.7, 1.2, 0.0, 0.45, ROCK_DK, n=7)
    prism(m, 0, 0, 1.2, 0.0, 0.2, ROCK, n=7)
    shards_full = [
        (0.0, 0.0, 0.62, 2.8, ORE_BODY),
        (0.85, 0.35, 0.4, 1.7, ORE_BODY),
        (-0.7, 0.55, 0.36, 1.5, (150, 226, 244)),
        (0.35, -0.8, 0.32, 1.3, (96, 196, 224)),
        (-0.55, -0.55, 0.26, 1.0, ORE_BODY),
        (0.7, -0.3, 0.22, 0.85, (150, 226, 244)),
    ]
    shards = shards_full if level == "FULL" else shards_full[:2]
    for cx, cz, r, h, body in shards:
        y0 = 0.2
        ymid = y0 + h * 0.55
        ytip = y0 + h
        prism(m, cx, cz, r, y0, ymid, body, n=5)
        pyramid(m, cx, cz, r, ymid, ytip, ORE_CORE, n=5)
        prism(m, cx, cz, r * 0.42, y0, ymid + 0.15, ORE_CORE, n=5, emissive=True)  # inner glow
        # a couple of bright glints near the tip.
        mote(m, cx + r * 0.4, ytip - h * 0.22, cz, 0.05, (255, 255, 255))
    return m


def carbon_node():
    # A vented rock mound with a glowing green fissure (smoke added in 2d after).
    m = []
    frustum(m, 0, 0, 2.0, 1.5, 0.0, 0.7, VENT_DK, n=8)
    frustum(m, 0, 0, 1.5, 1.1, 0.7, 1.4, VENT, n=8)
    # crooked vent rocks around the rim.
    for cx, cz in ((1.1, 0.4), (-0.7, 1.0), (-1.0, -0.7), (0.6, -1.0)):
        box(m, (cx - 0.28, 0.4, cz - 0.28), (cx + 0.28, 1.1 + 0.3 * cx, cz + 0.28), VENT)
    # glowing crater fissure.
    prism(m, 0, 0, 0.95, 1.4, 1.5, GAS_GLOW, n=8, emissive=True)
    prism(m, 0, 0, 0.65, 1.35, 1.52, (200, 255, 210), n=8, emissive=True)
    return m


def soft_circle(cv, cx, cy, r, color, a0):
    r2 = r * r
    for yy in range(int(cy - r), int(cy + r) + 1):
        for xx in range(int(cx - r), int(cx + r) + 1):
            d2 = (xx - cx) ** 2 + (yy - cy) ** 2
            if d2 <= r2:
                cv.blend(xx, yy, color, a0 * (1 - math.sqrt(d2 / r2)))


def draw_smoke(cv, x, y):
    # rising green vespene smoke: puffs grow + fade + drift as they climb.
    s = SS
    puffs = [
        (0.0, 0.0, 10, 0.55), (3, -16, 14, 0.46), (-4, -32, 18, 0.38),
        (6, -50, 22, 0.30), (-3, -70, 27, 0.22), (8, -92, 32, 0.15),
        (-2, -116, 38, 0.10),
    ]
    for dx, dy, r, a in puffs:
        col = (int(GAS_SMOKE[0] * 0.8 + 60), GAS_SMOKE[1], int(GAS_SMOKE[2] * 0.9 + 30))
        soft_circle(cv, x + dx * s, y + dy * s, r * s, col, a)


def main():
    os.makedirs(OUT_DIR, exist_ok=True)
    cols = 3
    cell = 300 * SS
    gap = 20 * SS
    margin = 36 * SS
    head = 38 * SS
    label_h = 30 * SS
    title_h = 64 * SS
    grid_w = cols * cell + (cols - 1) * gap
    W = margin * 2 + grid_w
    row_h = head + cell + label_h
    H = margin + title_h + 3 * row_h + margin // 2

    cv = Canvas(W, H)
    gradient_bg(cv)
    text(cv, margin, margin, "WORKERS + RESOURCE NODES  -  CONCEPT DRAFT", 3 * SS, (228, 234, 246))
    text(cv, margin, margin + 26 * SS,
         "POSES = WALK / BUILD / GATHER ANIM STATES  /  ROUGH MASSING",
         2 * SS, (120, 150, 190))

    rows = [
        ("ASTROMANCER ACOLYTE  -  HOVERS, GROWS, GATHERS", (176, 120, 240), A_VIO,
         [("IDLE / FLOAT", lambda: acolyte("IDLE"), True),
          ("GROW STRUCTURE", lambda: acolyte("GROW"), True),
          ("GATHER (MOTES)", lambda: acolyte("GATHER"), True)]),
        ("HOLLOWMEN ENGINEER  -  WALKS, WELDS, DRILLS", (230, 170, 80), H_HAZ,
         [("WALK", lambda: engineer("WALK"), False),
          ("BUILD (WELD)", lambda: engineer("BUILD"), False),
          ("GATHER (DRILL)", lambda: engineer("GATHER"), False)]),
        ("RESOURCE NODES  -  ORE CRYSTAL + CARBON GAS", (150, 220, 200), ORE_BODY,
         [("ORE  -  FULL", lambda: ore_node("FULL"), "ore"),
          ("ORE  -  DEPLETING", lambda: ore_node("LOW"), "ore"),
          ("CARBON  -  GAS GEYSER", carbon_node, "gas")]),
    ]

    y0 = margin + title_h
    for ri, (htext, hcol, accent, cells) in enumerate(rows):
        ry = y0 + ri * row_h
        cv.fill_rect(margin, ry, margin + grid_w, ry + head - 8 * SS, (18, 24, 38))
        cv.fill_rect(margin, ry, margin + 6 * SS, ry + head - 8 * SS, accent)
        text(cv, margin + 16 * SS, ry + 8 * SS, htext, 3 * SS, hcol)
        cy0 = ry + head
        for ci, (name, fn, kind) in enumerate(cells):
            x = margin + ci * (cell + gap)
            cv.fill_rect(x, cy0, x + cell, cy0 + cell, (13, 18, 30))
            for t in range(2 * SS):
                cv.fill_rect(x + t, cy0 + t, x + cell - t, cy0 + t + 1, (40, 52, 74))
                cv.fill_rect(x + t, cy0 + cell - t - 1, x + cell - t, cy0 + cell - t, (40, 52, 74))
                cv.fill_rect(x + t, cy0 + t, x + t + 1, cy0 + cell - t, (40, 52, 74))
                cv.fill_rect(x + cell - t - 1, cy0 + t, x + cell - t, cy0 + cell - t, (40, 52, 74))
            ccx = x + cell // 2
            ccy = cy0 + cell // 2 + 18 * SS
            if kind == "gas":
                render_object(cv, fn(), ccx, ccy, cell, cell, fit=0.78, ground_r=2.6,
                              shadow_r=2.4, shadow_a=0.55)
                draw_smoke(cv, ccx, ccy - 30 * SS)
            elif kind == "ore":
                render_object(cv, fn(), ccx, ccy, cell, cell, fit=0.8, ground_r=2.4, shadow_r=2.2)
            elif kind is True:  # hovering worker
                render_object(cv, fn(), ccx, ccy, cell, cell, fit=0.7, ground_r=1.5,
                              shadow_r=1.3, shadow_a=0.4)
            else:  # grounded worker
                render_object(cv, fn(), ccx, ccy, cell, cell, fit=0.7, ground_r=1.4, shadow_r=1.4)
            lw = text_w(name, 2 * SS)
            text(cv, x + (cell - lw) // 2, cy0 + cell + 6 * SS, name, 2 * SS, (208, 216, 230))

    fw, fh, out = downsample(cv)
    path = os.path.join(OUT_DIR, "units_resources.png")
    write_png(path, fw, fh, out)
    print("wrote", path, fw, "x", fh)


if __name__ == "__main__":
    main()
