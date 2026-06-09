#!/usr/bin/env python3
"""Concept renderer for faction buildings (no dependencies).

Builds rough, blocky low-poly building shapes from boxes / prisms / pyramids and
renders them with the game's fixed iso camera (yaw 45 deg, pitch ~54 deg) to a
single review sheet PNG. This is a design draft: approximate silhouettes and
proportions, not final art.

Run:  python3 assets/render_buildings.py
Out:  assets/concepts/buildings.png
"""

import os
import math
import struct
import zlib

OUT_DIR = os.path.join(os.path.dirname(__file__), "concepts")
SS = 2  # supersample factor (downsampled at the end)

# ---------------------------------------------------------------- PNG output ---


def write_png(path, w, h, rgba):
    def chunk(typ, data):
        return (
            struct.pack(">I", len(data))
            + typ
            + data
            + struct.pack(">I", zlib.crc32(typ + data) & 0xFFFFFFFF)
        )

    raw = bytearray()
    for y in range(h):
        raw.append(0)
        raw += rgba[y * w * 4 : (y + 1) * w * 4]
    out = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )
    with open(path, "wb") as f:
        f.write(out)


# ------------------------------------------------------------------ 3d math ---


def sub(a, b):
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def add(a, b):
    return (a[0] + b[0], a[1] + b[1], a[2] + b[2])


def cross(a, b):
    return (
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    )


def dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def norm(a):
    m = math.sqrt(dot(a, a)) or 1.0
    return (a[0] / m, a[1] / m, a[2] / m)


def newell(pts):
    n = [0.0, 0.0, 0.0]
    k = len(pts)
    for i in range(k):
        a = pts[i]
        b = pts[(i + 1) % k]
        n[0] += (a[1] - b[1]) * (a[2] + b[2])
        n[1] += (a[2] - b[2]) * (a[0] + b[0])
        n[2] += (a[0] - b[0]) * (a[1] + b[1])
    return norm((n[0], n[1], n[2]))


def centroid(pts):
    k = len(pts)
    return (
        sum(p[0] for p in pts) / k,
        sum(p[1] for p in pts) / k,
        sum(p[2] for p in pts) / k,
    )


# Camera basis (matches camera.rs: yaw 45, pitch 0.95 rad).
YAW = math.pi / 4.0
PITCH = 0.95
_d = norm((math.cos(YAW) * math.cos(PITCH), math.sin(PITCH), math.sin(YAW) * math.cos(PITCH)))
FWD = _d  # scene -> camera
RIGHT = norm(cross((0, 1, 0), FWD))
CAMUP = cross(FWD, RIGHT)
LIGHT = norm((-0.45, 0.92, 0.32))
FILL = norm((0.5, 0.25, -0.6))  # weak back-fill so shadowed faces stay readable


def view(p):
    return (dot(p, RIGHT), dot(p, CAMUP), dot(p, FWD))


# ---------------------------------------------------------------- shape kit ---
# A mesh is a list of faces: (pts3d, (r,g,b), emissive_bool).


def box(mesh, p0, p1, color, emissive=False):
    x0, y0, z0 = p0
    x1, y1, z1 = p1
    v = [
        (x0, y0, z0), (x1, y0, z0), (x1, y0, z1), (x0, y0, z1),
        (x0, y1, z0), (x1, y1, z0), (x1, y1, z1), (x0, y1, z1),
    ]
    quads = [
        (4, 5, 6, 7),  # top
        (0, 3, 2, 1),  # bottom
        (3, 7, 6, 2),  # +z
        (0, 1, 5, 4),  # -z
        (1, 2, 6, 5),  # +x
        (0, 4, 7, 3),  # -x
    ]
    for q in quads:
        mesh.append(([v[i] for i in q], color, emissive))


def ring(n, cx, cz, r, y, rot=0.0):
    return [
        (cx + r * math.cos(rot + 2 * math.pi * k / n), y, cz + r * math.sin(rot + 2 * math.pi * k / n))
        for k in range(n)
    ]


def prism(mesh, cx, cz, r, y0, y1, color, n=8, rot=0.0, top=True, bottom=False, emissive=False):
    lo = ring(n, cx, cz, r, y0, rot)
    hi = ring(n, cx, cz, r, y1, rot)
    for k in range(n):
        j = (k + 1) % n
        mesh.append(([lo[k], lo[j], hi[j], hi[k]], color, emissive))
    if top:
        mesh.append((list(reversed(hi)), color, emissive))
    if bottom:
        mesh.append((lo, color, emissive))


def frustum(mesh, cx, cz, r0, r1, y0, y1, color, n=8, rot=0.0, top=True, emissive=False):
    lo = ring(n, cx, cz, r0, y0, rot)
    hi = ring(n, cx, cz, r1, y1, rot)
    for k in range(n):
        j = (k + 1) % n
        mesh.append(([lo[k], lo[j], hi[j], hi[k]], color, emissive))
    if top:
        mesh.append((list(reversed(hi)), color, emissive))


def pyramid(mesh, cx, cz, r, y0, y1, color, n=4, rot=math.pi / 4, emissive=False):
    base = ring(n, cx, cz, r, y0, rot)
    apex = (cx, y1, cz)
    for k in range(n):
        j = (k + 1) % n
        mesh.append(([base[k], base[j], apex], color, emissive))


def crystal(mesh, cx, cz, r, y0, ymid, ytip, color, tip_color, n=6, rot=0.0):
    # short prism body + tapered point: an Astromancer grown shard.
    prism(mesh, cx, cz, r, y0, ymid, color, n=n, rot=rot, top=False)
    base = ring(n, cx, cz, r, ymid, rot)
    apex = (cx, ytip, cz)
    for k in range(n):
        j = (k + 1) % n
        mesh.append(([base[k], base[j], apex], tip_color, False))


def gable(mesh, x0, z0, x1, z1, y0, yk, color):
    # roof over [x0,x1]x[z0,z1], ridge along x at z-mid.
    zm = (z0 + z1) / 2.0
    a = (x0, y0, z0); b = (x1, y0, z0); c = (x1, y0, z1); d = (x0, y0, z1)
    r0 = (x0, yk, zm); r1 = (x1, yk, zm)
    mesh.append(([a, b, r1, r0], color, False))  # -z slope
    mesh.append(([d, r0, r1, c], color, False))  # +z slope
    mesh.append(([a, r0, d], color, False))      # gable -x
    mesh.append(([b, c, r1], color, False))      # gable +x


# ------------------------------------------------------------------ palette ---

# Astromancers: grown ivory shells, hovering, violet/teal energy, gold trim.
A_SHELL = (238, 233, 219)
A_SHELL2 = (216, 211, 194)
A_SHELL3 = (184, 179, 162)
A_VIO = (168, 92, 222)
A_TEAL = (96, 224, 210)
A_GOLD = (234, 204, 126)
A_COREV = (210, 150, 255)
A_CORET = (160, 255, 246)

# Hollowmen: gunmetal steel, hazard accents, amber/cyan windows, grounded guns.
H_STEEL = (124, 132, 142)
H_STEEL2 = (96, 104, 114)
H_DARK = (66, 72, 80)
H_PLATE = (158, 166, 174)
H_HAZ = (214, 158, 54)
H_AMBER = (250, 196, 96)
H_CYAN = (110, 196, 224)
H_GUN = (48, 52, 58)
H_RUST = (150, 96, 60)

HOVER = 1.1  # Astromancer ground gap


# --------------------------------------------------------------- buildings ---
# Each returns a mesh. Rough, characterful, ~footprint within +-5 on x/z.


def astro_spire():
    m = []
    y = HOVER
    # grown root: inverted faceted cone hanging beneath the hover gap.
    frustum(m, 0, 0, 2.2, 0.2, y, 0.05, A_SHELL3, n=8, top=False)
    # stacked tapering body.
    prism(m, 0, 0, 2.4, y, y + 3.4, A_SHELL, n=8)
    prism(m, 0, 0, 2.4 + 0.05, y + 1.5, y + 1.9, A_TEAL, n=8, emissive=True)  # energy band
    frustum(m, 0, 0, 1.9, 1.2, y + 3.4, y + 6.0, A_SHELL2, n=8)
    # crowning spire + core.
    pyramid(m, 0, 0, 1.2, y + 6.0, y + 9.2, A_GOLD, n=8)
    crystal(m, 0, 0, 0.55, y + 5.2, y + 7.0, y + 8.6, A_VIO, A_COREV, n=6)
    # orbiting shards.
    crystal(m, 3.0, 0.4, 0.5, y + 1.0, y + 2.0, y + 3.2, A_SHELL, A_COREV, n=6)
    crystal(m, -2.4, -1.6, 0.45, y + 0.6, y + 1.5, y + 2.6, A_SHELL, A_CORET, n=6)
    return m


def astro_reliquary():
    m = []
    y = HOVER
    # clean twin-post cradle holding a floating faceted relic-orb.
    frustum(m, 0, 0, 2.6, 1.8, y, y + 0.6, A_SHELL2, n=8)
    for sgn in (1, -1):
        box(m, (sgn * 1.9 - 0.24, y + 0.4, -0.3), (sgn * 1.9 + 0.24, y + 3.8, 0.3), A_SHELL)
    box(m, (-2.2, y + 3.5, -0.3), (2.2, y + 4.05, 0.3), A_GOLD)  # top yoke
    # the relic: a violet octahedral crystal, glowing.
    crystal(m, 0, 0, 1.2, y + 1.7, y + 2.6, y + 4.0, A_VIO, A_COREV, n=6)
    pyramid(m, 0, 0, 1.2, y + 2.6, y + 1.1, A_VIO, n=6)  # mirrored lower point
    prism(m, 0, 0, 1.25, y + 2.55, y + 2.7, A_CORET, n=6, emissive=True)
    return m


def astro_sanctum():
    m = []
    y = HOVER
    # broad grown hall: wide low frustum base + dome + side pods + a crystal.
    frustum(m, 0, 0, 4.6, 4.0, y, y + 0.5, A_SHELL3, n=8)
    box(m, (-4.0, y + 0.5, -3.2), (4.0, y + 2.4, 3.2), A_SHELL)
    prism(m, 0, 0, 3.2, y + 2.4, y + 2.7, A_GOLD, n=8)  # cornice
    frustum(m, 0, 0, 3.0, 0.4, y + 2.7, y + 4.6, A_SHELL2, n=8)  # dome-ish
    crystal(m, 0, 0, 0.7, y + 3.0, y + 4.4, y + 6.0, A_VIO, A_COREV, n=6)
    # glowing arched doorway + windows.
    box(m, (-1.1, y + 0.5, 3.18), (1.1, y + 2.0, 3.34), A_TEAL, emissive=True)
    for sx in (-2.7, 2.4):
        box(m, (sx, y + 1.0, 3.2), (sx + 0.3, y + 1.8, 3.32), A_CORET, emissive=True)
    # hovering side pods.
    for sgn in (1, -1):
        crystal(m, sgn * 4.4, 0, 0.7, y + 0.4, y + 1.2, y + 2.2, A_SHELL2, A_CORET, n=6)
    return m


def astro_citadel():
    m = []
    y = HOVER
    # defensive temple-fortress (lifts off as the Flying Fortress): stepped,
    # central spire, four crystal horns.
    frustum(m, 0, 0, 4.8, 4.2, y, y + 0.6, A_SHELL3, n=8)
    box(m, (-4.0, y + 0.6, -4.0), (4.0, y + 2.2, 4.0), A_SHELL2)
    box(m, (-3.0, y + 2.2, -3.0), (3.0, y + 3.6, 3.0), A_SHELL)
    prism(m, 0, 0, 2.2, y + 3.6, y + 6.4, A_SHELL2, n=8)
    pyramid(m, 0, 0, 1.6, y + 6.4, y + 9.0, A_GOLD, n=8)
    crystal(m, 0, 0, 0.6, y + 5.4, y + 7.2, y + 8.6, A_VIO, A_COREV, n=6)
    # corner battle-horns (the temple's guns).
    for cx, cz in ((3.3, 3.3), (-3.3, 3.3), (3.3, -3.3), (-3.3, -3.3)):
        crystal(m, cx, cz, 0.6, y + 2.2, y + 3.4, y + 5.0, A_SHELL, A_CORET, n=6)
    box(m, (-4.0, y + 1.6, -4.0), (4.0, y + 1.9, 4.0), A_GOLD)  # belt
    return m


def astro_skycradle():
    m = []
    y = HOVER
    # an open bowl-cradle growing a flying-castle pod, ringed by gold-tipped
    # spires and energy tethers.
    frustum(m, 0, 0, 4.4, 3.4, y, y + 0.6, A_SHELL3, n=8)
    # the cradle: a wide cup opening upward (open top) that cups the pod.
    frustum(m, 0, 0, 1.9, 4.0, y + 0.5, y + 3.0, A_SHELL2, n=8, top=False)
    prism(m, 0, 0, 4.0, y + 2.7, y + 3.0, A_GOLD, n=8, top=False)  # gold rim
    # the half-grown castle pod suspended in the cup.
    crystal(m, 0, 0, 1.8, y + 1.8, y + 3.6, y + 5.6, A_VIO, A_COREV, n=6)
    pyramid(m, 0, 0, 1.8, y + 1.8, y + 0.7, A_VIO, n=6)
    prism(m, 0, 0, 1.85, y + 2.5, y + 2.7, A_CORET, n=6, emissive=True)
    # four gold-tipped guard spires around the rim + glowing tethers.
    for cx, cz in ((3.4, 3.4), (-3.4, 3.4), (3.4, -3.4), (-3.4, -3.4)):
        prism(m, cx, cz, 0.4, y + 0.6, y + 3.6, A_SHELL, n=6, top=False)
        pyramid(m, cx, cz, 0.4, y + 3.6, y + 4.6, A_GOLD, n=6)
        box(m, (cx - 0.08, y + 2.0, cz - 0.08), (cx + 0.08, y + 3.4, cz + 0.08), A_TEAL, emissive=True)
    return m


def holl_command():
    m = []
    # broad armored command center, raised control tower, antenna + roof turret.
    box(m, (-4.6, 0.0, -4.0), (4.6, 0.4, 4.0), H_DARK)
    box(m, (-4.2, 0.4, -3.6), (4.2, 2.6, 3.6), H_STEEL)
    box(m, (-4.2, 2.6, -3.6), (4.2, 3.0, 3.6), H_STEEL2)  # roof lip
    # angled corner armor.
    for sgn in (1, -1):
        box(m, (sgn * 4.2, 0.4, -1.2), (sgn * 4.7, 2.4, 1.2), H_PLATE)
    # control tower.
    box(m, (-1.8, 3.0, -1.8), (1.8, 5.2, 1.8), H_STEEL2)
    box(m, (-1.9, 4.0, -1.9), (1.9, 4.7, 1.9), H_CYAN, emissive=True)  # window band
    box(m, (-2.0, 5.2, -2.0), (2.0, 5.5, 2.0), H_DARK)
    # roof turret (built-in gun).
    box(m, (-0.7, 5.5, -0.7), (0.7, 6.1, 0.7), H_GUN)
    box(m, (-0.25, 5.7, 0.3), (0.25, 6.0, 3.0), H_GUN)  # barrel
    # antenna mast + hazard stripe.
    box(m, (3.0, 3.0, 2.6), (3.2, 6.2, 2.8), H_STEEL2)
    box(m, (-4.2, 0.4, -3.6), (4.2, 0.8, 3.6), H_HAZ)  # base stripe
    box(m, (-1.6, 0.4, 3.6), (1.6, 2.2, 3.74), H_DARK)  # door
    return m


def holl_refinery():
    m = []
    # blocky plant hall + two storage tanks + pipes + a vent stack.
    box(m, (-4.6, 0.0, -3.6), (1.2, 0.4, 3.6), H_DARK)
    box(m, (-4.2, 0.4, -3.2), (0.8, 2.6, 3.2), H_STEEL)
    gable(m, -4.2, -3.2, 0.8, 3.2, 2.6, 3.5, H_STEEL2)
    # storage tanks.
    prism(m, 2.8, 1.7, 1.7, 0.0, 3.4, H_PLATE, n=10)
    prism(m, 2.8, 1.7, 1.75, 1.4, 1.7, H_HAZ, n=10)  # hazard band
    prism(m, 3.0, -2.0, 1.3, 0.0, 2.6, H_STEEL2, n=10)
    # connecting pipes.
    box(m, (0.8, 1.0, 1.4), (2.0, 1.3, 2.0), H_STEEL2)
    box(m, (0.8, 0.7, -2.2), (2.0, 1.0, -1.8), H_STEEL2)
    # vent stack with glow.
    box(m, (-3.4, 2.6, -2.2), (-2.6, 5.0, -1.4), H_STEEL2)
    box(m, (-3.4, 4.6, -2.2), (-2.6, 5.0, -1.4), H_AMBER, emissive=True)
    box(m, (-4.2, 0.4, -3.2), (0.8, 0.8, 3.2), H_HAZ)
    return m


def holl_reactor():
    m = []
    # squat containment with a domed core + two cooling stacks + glow vents.
    box(m, (-3.8, 0.0, -3.8), (3.8, 0.5, 3.8), H_DARK)
    box(m, (-3.4, 0.5, -3.4), (3.4, 2.2, 3.4), H_STEEL)
    # containment dome.
    prism(m, 0, 0, 2.6, 2.2, 2.6, H_STEEL2, n=12)
    frustum(m, 0, 0, 2.6, 1.6, 2.6, 3.8, H_PLATE, n=12)
    prism(m, 0, 0, 0.8, 3.8, 4.4, H_CYAN, n=8, emissive=True)  # core glow
    # cooling stacks (flared).
    for cx, cz in ((-2.6, 2.6), (2.6, -2.6)):
        frustum(m, cx, cz, 0.7, 1.0, 0.5, 4.6, H_STEEL2, n=8)
        prism(m, cx, cz, 1.0, 4.4, 4.7, H_DARK, n=8)
    # glowing vents on the body.
    for sx in (-2.4, 0.0, 2.4):
        box(m, (sx - 0.5, 0.9, 3.4), (sx + 0.5, 1.7, 3.5), H_AMBER, emissive=True)
    box(m, (-3.4, 0.5, -3.4), (3.4, 0.9, 3.4), H_HAZ)
    return m


def holl_factory():
    m = []
    # wide hangar, sawtooth roof, roll-up door, smokestacks, crane, side turret.
    box(m, (-5.0, 0.0, -3.8), (4.0, 0.4, 3.8), H_DARK)
    box(m, (-4.6, 0.4, -3.4), (3.6, 3.0, 3.4), H_STEEL)
    # sawtooth roof segments.
    for sx in (-4.6, -2.3, 0.0, 2.3):
        gable(m, sx, -3.4, sx + 2.3, 3.4, 3.0, 3.7, H_STEEL2)
    # big roll-up door + hazard frame.
    box(m, (-2.4, 0.4, 3.4), (2.0, 2.6, 3.52), H_STEEL2)
    box(m, (-2.2, 0.6, 3.46), (1.8, 2.4, 3.56), H_HAZ)
    box(m, (-2.0, 0.6, 3.5), (1.6, 2.3, 3.6), H_GUN)
    # smokestacks.
    for cx in (-3.8, 3.0):
        prism(m, cx, -2.6, 0.45, 3.0, 5.4, H_STEEL2, n=6)
        prism(m, cx, -2.6, 0.45, 5.1, 5.4, H_DARK, n=6)
    # crane arm.
    box(m, (3.6, 2.8, 0.0), (3.8, 4.6, 0.3), H_STEEL2)
    box(m, (1.8, 4.3, 0.05), (3.9, 4.6, 0.25), H_HAZ)
    # corner gun.
    box(m, (-4.7, 3.0, -3.0), (-3.7, 3.8, -2.0), H_GUN)
    box(m, (-4.5, 3.3, -3.6), (-3.9, 3.6, -2.4), H_GUN)
    box(m, (-4.6, 0.4, -3.4), (3.6, 0.8, 3.4), H_HAZ)
    return m


def holl_nullpylon():
    m = []
    # armored base + tall emitter mast with a coil/dish and a base gun.
    box(m, (-2.6, 0.0, -2.6), (2.6, 0.5, 2.6), H_DARK)
    frustum(m, 0, 0, 2.4, 1.6, 0.5, 2.0, H_STEEL, n=6)
    box(m, (-2.0, 0.5, -2.0), (2.0, 0.9, 2.0), H_HAZ)
    # mast.
    prism(m, 0, 0, 0.8, 2.0, 6.0, H_STEEL2, n=6)
    # null-field emitter rings (glow), anti-magic cyan-violet.
    for yy in (3.0, 4.0, 5.0):
        prism(m, 0, 0, 1.2, yy, yy + 0.28, (150, 110, 220), n=8, emissive=True)
    # emitter head.
    frustum(m, 0, 0, 0.8, 1.5, 6.0, 6.8, H_PLATE, n=6)
    prism(m, 0, 0, 0.9, 6.8, 7.4, (170, 130, 240), n=8, emissive=True)
    pyramid(m, 0, 0, 1.5, 6.8, 8.2, H_STEEL2, n=6)
    # base gun.
    box(m, (1.4, 0.9, 1.4), (2.2, 1.7, 2.2), H_GUN)
    box(m, (1.6, 1.1, 1.9), (2.0, 1.5, 3.2), H_GUN)
    return m


ASTRO = [
    ("SPIRE (HQ)", astro_spire),
    ("RELIQUARY", astro_reliquary),
    ("SANCTUM", astro_sanctum),
    ("CITADEL", astro_citadel),
    ("SKY-CRADLE", astro_skycradle),
]
HOLLOW = [
    ("COMMAND HQ", holl_command),
    ("REFINERY", holl_refinery),
    ("REACTOR", holl_reactor),
    ("FACTORY", holl_factory),
    ("NULL PYLON", holl_nullpylon),
]


# ------------------------------------------------------------------ raster ---


class Canvas:
    def __init__(self, w, h):
        self.w = w
        self.h = h
        self.buf = bytearray(w * h * 3)

    def set(self, x, y, rgb):
        if 0 <= x < self.w and 0 <= y < self.h:
            o = (y * self.w + x) * 3
            self.buf[o] = rgb[0]
            self.buf[o + 1] = rgb[1]
            self.buf[o + 2] = rgb[2]

    def blend(self, x, y, rgb, a):
        if 0 <= x < self.w and 0 <= y < self.h:
            o = (y * self.w + x) * 3
            self.buf[o] = int(self.buf[o] * (1 - a) + rgb[0] * a)
            self.buf[o + 1] = int(self.buf[o + 1] * (1 - a) + rgb[1] * a)
            self.buf[o + 2] = int(self.buf[o + 2] * (1 - a) + rgb[2] * a)

    def fill_rect(self, x0, y0, x1, y1, rgb):
        for y in range(max(0, y0), min(self.h, y1)):
            o = (y * self.w + max(0, x0)) * 3
            for x in range(max(0, x0), min(self.w, x1)):
                self.buf[o] = rgb[0]
                self.buf[o + 1] = rgb[1]
                self.buf[o + 2] = rgb[2]
                o += 3


def clampc(c):
    return (
        0 if c[0] < 0 else 255 if c[0] > 255 else int(c[0]),
        0 if c[1] < 0 else 255 if c[1] > 255 else int(c[1]),
        0 if c[2] < 0 else 255 if c[2] > 255 else int(c[2]),
    )


def fill_poly(cv, pts, rgb):
    if len(pts) < 3:
        return
    ys = [p[1] for p in pts]
    y0 = max(0, int(math.floor(min(ys))))
    y1 = min(cv.h - 1, int(math.ceil(max(ys))))
    n = len(pts)
    for y in range(y0, y1 + 1):
        yc = y + 0.5
        xs = []
        for i in range(n):
            ax, ay = pts[i]
            bx, by = pts[(i + 1) % n]
            if (ay <= yc < by) or (by <= yc < ay):
                t = (yc - ay) / (by - ay)
                xs.append(ax + t * (bx - ax))
        if len(xs) < 2:
            continue
        xs.sort()
        for k in range(0, len(xs) - 1, 2):
            xa = int(math.ceil(xs[k] - 0.5))
            xb = int(math.floor(xs[k + 1] - 0.5))
            o = (y * cv.w + max(0, xa)) * 3
            for x in range(max(0, xa), min(cv.w - 1, xb) + 1):
                cv.buf[o] = rgb[0]
                cv.buf[o + 1] = rgb[1]
                cv.buf[o + 2] = rgb[2]
                o += 3


def draw_line(cv, x0, y0, x1, y1, rgb):
    x0, y0, x1, y1 = int(x0), int(y0), int(x1), int(y1)
    dx = abs(x1 - x0)
    dy = -abs(y1 - y0)
    sx = 1 if x0 < x1 else -1
    sy = 1 if y0 < y1 else -1
    err = dx + dy
    while True:
        cv.set(x0, y0, rgb)
        if x0 == x1 and y0 == y1:
            break
        e2 = 2 * err
        if e2 >= dy:
            err += dy
            x0 += sx
        if e2 <= dx:
            err += dx
            y0 += sy


def shade(color, normal):
    diff = max(0.0, dot(normal, LIGHT))
    fill = max(0.0, dot(normal, FILL)) * 0.18
    inten = 0.40 + 0.72 * diff + fill
    return clampc((color[0] * inten, color[1] * inten, color[2] * inten))


def render_building(cv, mesh, cx, cy, cell_w, cell_h):
    # auto-fit: project all verts to view plane, scale to cell.
    pv = []
    bc = centroid([p for f in mesh for p in f[0]])
    vmin = [1e9, 1e9]
    vmax = [-1e9, -1e9]
    for f in mesh:
        for p in f[0]:
            vx, vy, _ = view(p)
            vmin[0] = min(vmin[0], vx); vmax[0] = max(vmax[0], vx)
            vmin[1] = min(vmin[1], vy); vmax[1] = max(vmax[1], vy)
    bw = (vmax[0] - vmin[0]) or 1.0
    bh = (vmax[1] - vmin[1]) or 1.0
    scale = min(cell_w * 0.84 / bw, cell_h * 0.84 / bh)
    mx = (vmin[0] + vmax[0]) / 2.0
    my = (vmin[1] + vmax[1]) / 2.0

    def proj(p):
        vx, vy, vz = view(p)
        return ((vx - mx) * scale + cx, cy - (vy - my) * scale, vz)

    # ground shadow ellipse under the footprint.
    gv = proj((0, 0, 0))
    rxs = scale * 4.6
    rys = scale * 4.6 * abs(RIGHT[1] - 0) + scale * 2.3
    rys = scale * 2.4
    for yy in range(int(gv[1] - rys), int(gv[1] + rys)):
        dy = (yy - gv[1]) / rys
        if abs(dy) > 1:
            continue
        half = rxs * math.sqrt(max(0.0, 1 - dy * dy))
        for xx in range(int(gv[0] - half), int(gv[0] + half)):
            cv.blend(xx, yy, (6, 9, 16), 0.5)

    faces = []
    for pts, color, emis in mesh:
        nrm = newell(pts)
        fc = centroid(pts)
        if dot(nrm, sub(fc, bc)) < 0:
            nrm = (-nrm[0], -nrm[1], -nrm[2])
        if not emis and dot(nrm, FWD) <= 0.02:
            continue  # backface
        proj_pts = [proj(p) for p in pts]
        depth = sum(p[2] for p in proj_pts) / len(proj_pts)
        col = color if emis else shade(color, nrm)
        faces.append((depth, [(p[0], p[1]) for p in proj_pts], col, emis))
    faces.sort(key=lambda f: f[0])
    for _, p2, col, emis in faces:
        fill_poly(cv, p2, col)
        edge = (min(255, col[0] + 40), min(255, col[1] + 40), min(255, col[2] + 40)) if emis \
            else (int(col[0] * 0.55), int(col[1] * 0.55), int(col[2] * 0.55))
        for i in range(len(p2)):
            a = p2[i]; b = p2[(i + 1) % len(p2)]
            draw_line(cv, a[0], a[1], b[0], b[1], edge)


# -------------------------------------------------------------------- font ---

FONT = {
    "A": ["01110", "10001", "10001", "11111", "10001", "10001", "10001"],
    "B": ["11110", "10001", "10001", "11110", "10001", "10001", "11110"],
    "C": ["01110", "10001", "10000", "10000", "10000", "10001", "01110"],
    "D": ["11110", "10001", "10001", "10001", "10001", "10001", "11110"],
    "E": ["11111", "10000", "10000", "11110", "10000", "10000", "11111"],
    "F": ["11111", "10000", "10000", "11110", "10000", "10000", "10000"],
    "G": ["01110", "10001", "10000", "10111", "10001", "10001", "01110"],
    "H": ["10001", "10001", "10001", "11111", "10001", "10001", "10001"],
    "I": ["11111", "00100", "00100", "00100", "00100", "00100", "11111"],
    "J": ["00111", "00010", "00010", "00010", "10010", "10010", "01100"],
    "K": ["10001", "10010", "10100", "11000", "10100", "10010", "10001"],
    "L": ["10000", "10000", "10000", "10000", "10000", "10000", "11111"],
    "M": ["10001", "11011", "10101", "10101", "10001", "10001", "10001"],
    "N": ["10001", "11001", "10101", "10101", "10011", "10001", "10001"],
    "O": ["01110", "10001", "10001", "10001", "10001", "10001", "01110"],
    "P": ["11110", "10001", "10001", "11110", "10000", "10000", "10000"],
    "Q": ["01110", "10001", "10001", "10001", "10101", "10010", "01101"],
    "R": ["11110", "10001", "10001", "11110", "10100", "10010", "10001"],
    "S": ["01111", "10000", "10000", "01110", "00001", "00001", "11110"],
    "T": ["11111", "00100", "00100", "00100", "00100", "00100", "00100"],
    "U": ["10001", "10001", "10001", "10001", "10001", "10001", "01110"],
    "V": ["10001", "10001", "10001", "10001", "10001", "01010", "00100"],
    "W": ["10001", "10001", "10001", "10101", "10101", "11011", "10001"],
    "X": ["10001", "10001", "01010", "00100", "01010", "10001", "10001"],
    "Y": ["10001", "10001", "01010", "00100", "00100", "00100", "00100"],
    "Z": ["11111", "00001", "00010", "00100", "01000", "10000", "11111"],
    "-": ["00000", "00000", "00000", "11111", "00000", "00000", "00000"],
    "(": ["00100", "01000", "01000", "01000", "01000", "01000", "00100"],
    ")": ["00100", "00010", "00010", "00010", "00010", "00010", "00100"],
    "/": ["00001", "00001", "00010", "00100", "01000", "10000", "10000"],
    " ": ["00000", "00000", "00000", "00000", "00000", "00000", "00000"],
}


def text(cv, x, y, s, scale, rgb):
    cx = x
    for ch in s.upper():
        g = FONT.get(ch, FONT[" "])
        for ry in range(7):
            for rx in range(5):
                if g[ry][rx] == "1":
                    cv.fill_rect(cx + rx * scale, y + ry * scale,
                                 cx + rx * scale + scale, y + ry * scale + scale, rgb)
        cx += 6 * scale


def text_w(s, scale):
    return len(s) * 6 * scale


# ------------------------------------------------------------------- sheet ---


def main():
    os.makedirs(OUT_DIR, exist_ok=True)
    cols = 5
    cell = 300 * SS
    gap = 20 * SS
    margin = 36 * SS
    head = 38 * SS
    label_h = 30 * SS
    title_h = 64 * SS

    grid_w = cols * cell + (cols - 1) * gap
    W = margin * 2 + grid_w
    row_h = head + cell + label_h
    H = margin + title_h + 2 * row_h + margin // 2

    cv = Canvas(W, H)
    # background: vertical gradient, deep space blue.
    for y in range(H):
        t = y / H
        c = (int(8 + 6 * (1 - t)), int(12 + 10 * (1 - t)), int(22 + 16 * (1 - t)))
        cv.fill_rect(0, y, W, y + 1, c)

    text(cv, margin, margin, "FACTION BUILDINGS  -  CONCEPT DRAFT", 4 * SS, (228, 234, 246))
    text(cv, margin, margin + 30 * SS,
         "ROUGH MASSING ONLY  /  GROWN-CRYSTAL VS INDUSTRIAL-STEEL  /  GAME ISO CAMERA",
         2 * SS, (120, 150, 190))

    rows = [
        ("ASTROMANCERS  -  GROWN, HOVERING, MAGED", (176, 120, 240), A_VIO, ASTRO),
        ("HOLLOWMEN  -  INDUSTRIAL, ARMORED, GROUNDED", (230, 170, 80), H_HAZ, HOLLOW),
    ]
    y0 = margin + title_h
    for ri, (htext, hcol, accent, blds) in enumerate(rows):
        ry = y0 + ri * row_h
        # row header band.
        cv.fill_rect(margin, ry, margin + grid_w, ry + head - 8 * SS, (18, 24, 38))
        cv.fill_rect(margin, ry, margin + 6 * SS, ry + head - 8 * SS, accent)
        text(cv, margin + 16 * SS, ry + 8 * SS, htext, 3 * SS, hcol)
        cy0 = ry + head
        for ci, (name, fn) in enumerate(blds):
            x = margin + ci * (cell + gap)
            # cell panel.
            cv.fill_rect(x, cy0, x + cell, cy0 + cell, (13, 18, 30))
            for t in range(2 * SS):
                cv.fill_rect(x + t, cy0 + t, x + cell - t, cy0 + t + 1, (40, 52, 74))
                cv.fill_rect(x + t, cy0 + cell - t - 1, x + cell - t, cy0 + cell - t, (40, 52, 74))
                cv.fill_rect(x + t, cy0 + t, x + t + 1, cy0 + cell - t, (40, 52, 74))
                cv.fill_rect(x + cell - t - 1, cy0 + t, x + cell - t, cy0 + cell - t, (40, 52, 74))
            render_building(cv, fn(), x + cell // 2, cy0 + cell // 2 + 22 * SS, cell, cell)
            lw = text_w(name, 2 * SS)
            text(cv, x + (cell - lw) // 2, cy0 + cell + 6 * SS, name, 2 * SS, (208, 216, 230))

    # downsample SS -> 1 (box filter).
    fw, fh = W // SS, H // SS
    out = bytearray(fw * fh * 4)
    for y in range(fh):
        for x in range(fw):
            r = g = b = 0
            for sy in range(SS):
                for sx in range(SS):
                    o = ((y * SS + sy) * W + (x * SS + sx)) * 3
                    r += cv.buf[o]; g += cv.buf[o + 1]; b += cv.buf[o + 2]
            n = SS * SS
            o = (y * fw + x) * 4
            out[o] = r // n; out[o + 1] = g // n; out[o + 2] = b // n; out[o + 3] = 255
    path = os.path.join(OUT_DIR, "buildings.png")
    write_png(path, fw, fh, out)
    print("wrote", path, fw, "x", fh)


if __name__ == "__main__":
    main()
