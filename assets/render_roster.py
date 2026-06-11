#!/usr/bin/env python3
"""Concept renderer for the proposed roster expansion (no dependencies).

Draws design sketches for the new Astromancer caster college (Pyromancer,
Stormcaller, Hex-Witch, Druid, Evoker, Chronomancer, Seer, Wisp, Tempest Dais,
Athenaeum, Storm-Ward) and the new Hollowmen chassis families (Hound, Javelin,
Wrecker, Bulwark, Earthshaker, Hailstorm, Interceptor, Vulture, Arsenal,
Bunker, Flak Tower) in the game's iso camera. Rough massing and silhouette
intent only, not final art.

Run:  python3 assets/render_roster.py
Out:  assets/concepts/roster_astro.png
      assets/concepts/roster_hollow.png
"""

import os
import math

from concept_kit import (
    SS, Canvas, box, prism, frustum, pyramid, crystal, ring, render_object,
    downsample, write_png, gradient_bg, text, text_w,
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
EMBER = (252, 120, 56)
FLAME = (255, 196, 92)
FIRE_CORE = (255, 242, 200)
STORM = (120, 200, 255)
BOLTC = (214, 240, 255)
HEXG = (150, 255, 130)
WITCH_ROBE = (122, 86, 160)
WITCH_ROBE2 = (96, 66, 130)
MOSS = (142, 176, 98)
MOSS2 = (112, 142, 76)
BARK = (110, 84, 58)
BLOOM = (120, 255, 140)
SAND = (255, 224, 140)
CLOUD = (152, 160, 174)
# Hollowmen (industrial, grounded).
H_STEEL = (124, 132, 142)
H_STEEL2 = (160, 168, 176)
H_DARK = (60, 66, 74)
H_HAZ = (214, 158, 54)
H_AMBER = (252, 200, 98)
H_CYAN = (112, 198, 226)
H_GUN = (46, 50, 56)
H_RED = (255, 96, 72)
CONCRETE = (142, 144, 140)
CONCRETE2 = (114, 116, 112)


def mote(mesh, x, y, z, s, color):
    box(mesh, (x - s, y - s, z - s), (x + s, y + s, z + s), color, emissive=True)


def octa(mesh, x, y, z, s, color):
    """A floating sigil: two pyramids base to base."""
    pyramid(mesh, x, z, s, y, y + s * 1.5, color, emissive=True)
    pyramid(mesh, x, z, s, y, y - s * 1.5, color, emissive=True)


def bolt(mesh, x, z, y0, y1, color, seg=4, amp=0.18, s=0.055):
    """A dashed zigzag lightning stroke between two heights."""
    for k in range(seg):
        ya = y0 + (y1 - y0) * k / seg
        yb = y0 + (y1 - y0) * (k + 0.8) / seg
        ox = amp if k % 2 == 0 else -amp
        box(mesh, (x + ox - s, ya, z - s), (x + ox + s, yb, z + s), color, emissive=True)


def cloud(mesh, cx, cz, y, scale=1.0):
    for dx, dz, r in ((0.0, 0.0, 0.5), (0.45, 0.18, 0.34), (-0.42, -0.12, 0.3)):
        prism(mesh, cx + dx * scale, cz + dz * scale, r * scale, y, y + 0.34 * scale, CLOUD, n=6)


def caster_base(mesh, yb, robe, robe2, sash, hooded=True, eyes=A_CORET):
    """The shared hovering-robe body all Astromancer casters build on."""
    frustum(mesh, 0, 0, 0.18, 0.62, yb - 0.55, yb + 0.05, robe2, n=6, top=False)
    frustum(mesh, 0, 0, 0.62, 0.46, yb + 0.05, yb + 1.0, robe, n=6)
    box(mesh, (-0.5, yb + 0.55, -0.12), (0.5, yb + 0.72, 0.12), sash)
    frustum(mesh, 0, 0, 0.5, 0.34, yb + 1.0, yb + 1.35, robe2, n=6)
    frustum(mesh, 0, 0, 0.34, 0.26, yb + 1.35, yb + 1.7, robe, n=6)
    if hooded:
        pyramid(mesh, 0, 0, 0.3, yb + 1.55, yb + 2.0, robe2, n=6)
        box(mesh, (-0.16, yb + 1.42, 0.2), (0.16, yb + 1.56, 0.34), eyes, emissive=True)
    else:
        box(mesh, (-0.16, yb + 1.46, 0.18), (0.16, yb + 1.6, 0.32), eyes, emissive=True)


# ------------------------------------------------------------- Astro casters


def pyromancer():
    m = []
    yb = 0.85
    caster_base(m, yb, A_SHELL, A_SHELL2, EMBER, eyes=FLAME)
    # hood horns.
    pyramid(m, -0.3, 0.0, 0.1, yb + 1.85, yb + 2.25, A_GOLD, n=4)
    pyramid(m, 0.3, 0.0, 0.1, yb + 1.85, yb + 2.25, A_GOLD, n=4)
    # brazier staff: dark haft, gold bowl, a tall flame.
    box(m, (0.52, yb - 0.2, 0.24), (0.6, yb + 2.1, 0.32), BARK)
    frustum(m, 0.56, 0.28, 0.1, 0.3, yb + 2.1, yb + 2.32, A_GOLD, n=6)
    pyramid(m, 0.56, 0.28, 0.22, yb + 2.32, yb + 3.05, FLAME, n=6, emissive=True)
    pyramid(m, 0.56, 0.28, 0.1, yb + 2.36, yb + 2.8, FIRE_CORE, n=6, emissive=True)
    # off-hand fireball plus rising sparks.
    prism(m, -0.6, 0.34, 0.2, yb + 0.9, yb + 1.26, EMBER, n=6, emissive=True)
    for i, t in enumerate((0.0, 0.4, 0.8)):
        mote(m, -0.6 + 0.18 * t, yb + 1.4 + 0.55 * t, 0.34 - 0.1 * t, 0.06, FLAME)
    return m


def stormcaller():
    m = []
    yb = 0.85
    caster_base(m, yb, A_SHELL, A_SHELL2, STORM, hooded=False, eyes=BOLTC)
    # tall conical hat.
    frustum(m, 0, 0, 0.5, 0.44, yb + 1.7, yb + 1.8, A_SHELL2, n=6)
    pyramid(m, 0, 0, 0.4, yb + 1.8, yb + 2.85, A_SHELL2, n=6)
    mote(m, 0, yb + 2.85, 0, 0.06, STORM)
    # staff crowned with a glowing storm disc.
    box(m, (0.52, yb - 0.2, 0.24), (0.6, yb + 1.9, 0.32), BARK)
    prism(m, 0.56, 0.28, 0.3, yb + 1.9, yb + 2.0, STORM, n=8, emissive=True)
    # a called-in storm: cloud off to the side, striking the ground.
    cloud(m, 1.45, 0.75, yb + 3.1, scale=1.0)
    bolt(m, 1.45, 0.75, 0.0, yb + 3.1, BOLTC, seg=5, amp=0.18, s=0.06)
    mote(m, 1.45, 0.08, 0.75, 0.1, BOLTC)
    return m


def hex_witch():
    m = []
    yb = 0.85
    caster_base(m, yb, WITCH_ROBE, WITCH_ROBE2, (52, 40, 72), hooded=False, eyes=HEXG)
    # wide-brim hat with a crooked, two-step cone.
    frustum(m, 0, 0, 0.78, 0.7, yb + 1.66, yb + 1.76, WITCH_ROBE2, n=7)
    pyramid(m, 0, 0, 0.34, yb + 1.76, yb + 2.2, WITCH_ROBE2, n=6)
    pyramid(m, 0.14, 0.0, 0.18, yb + 2.08, yb + 2.55, WITCH_ROBE, n=6)
    # crooked staff.
    box(m, (-0.6, yb - 0.2, 0.24), (-0.52, yb + 1.6, 0.32), BARK)
    box(m, (-0.68, yb + 1.6, 0.22), (-0.5, yb + 1.78, 0.34), BARK)
    octa(m, -0.59, yb + 2.0, 0.28, 0.13, HEXG)
    # orbiting curse sigils.
    octa(m, 0.85, yb + 0.6, 0.4, 0.11, HEXG)
    octa(m, 0.7, yb + 1.5, -0.45, 0.11, A_COREV)
    octa(m, -0.5, yb + 0.4, -0.7, 0.11, HEXG)
    return m


def druid():
    m = []
    yb = 0.85
    caster_base(m, yb, MOSS, MOSS2, BARK, eyes=BLOOM)
    # antlers branching off the hood.
    for sx in (-1, 1):
        box(m, (sx * 0.34 - 0.04, yb + 1.7, -0.04), (sx * 0.34 + 0.04, yb + 2.3, 0.04), BARK)
        box(m, (sx * 0.34 - 0.04, yb + 2.1, -0.04), (sx * 0.56 + 0.02, yb + 2.18, 0.04), BARK)
        box(m, (sx * 0.52 - 0.04, yb + 2.18, -0.04), (sx * 0.52 + 0.04, yb + 2.5, 0.04), BARK)
    # living staff with a leaf-crystal head.
    box(m, (0.52, yb - 0.2, 0.24), (0.6, yb + 1.95, 0.32), BARK)
    crystal(m, 0.56, 0.28, 0.16, yb + 1.95, yb + 2.3, yb + 2.7, A_TEAL, BLOOM, n=5)
    # a blooming irradiated patch: glowing ground, sprouting shards, motes.
    prism(m, 1.0, 0.85, 0.66, 0.0, 0.07, BLOOM, n=8, emissive=True)
    crystal(m, 0.85, 0.7, 0.14, 0.07, 0.4, 0.72, MOSS, BLOOM, n=5)
    crystal(m, 1.2, 1.0, 0.1, 0.07, 0.3, 0.55, MOSS, BLOOM, n=5)
    for i, t in enumerate((0.0, 0.45, 0.9)):
        mote(m, 1.0 - 0.2 * t, 0.25 + 0.7 * t, 0.85 - 0.2 * t, 0.055, BLOOM)
    return m


def evoker():
    m = []
    yb = 0.85
    caster_base(m, yb, A_SHELL, A_SHELL2, A_VIO)
    # focus orb raised above the hood, mid-summons.
    prism(m, 0, 0, 0.2, yb + 2.1, yb + 2.45, A_COREV, n=6, emissive=True)
    # two aether constructs condensing on summoning circles.
    for sx in (-1, 1):
        cx = sx * 1.05
        cz = 0.55
        prism(m, cx, cz, 0.5, 0.0, 0.06, A_COREV, n=8, emissive=True)
        hov = 0.45
        box(m, (cx - 0.22, hov, cz - 0.16), (cx + 0.22, hov + 0.52, cz + 0.16), A_TEAL,
            emissive=True)
        box(m, (cx - 0.13, hov + 0.58, cz - 0.12), (cx + 0.13, hov + 0.82, cz + 0.12), A_TEAL,
            emissive=True)
        box(m, (cx - 0.34, hov + 0.18, cz - 0.1), (cx - 0.22, hov + 0.5, cz + 0.1), A_CORET,
            emissive=True)
        box(m, (cx + 0.22, hov + 0.18, cz - 0.1), (cx + 0.34, hov + 0.5, cz + 0.1), A_CORET,
            emissive=True)
        mote(m, cx, hov + 1.0, cz, 0.06, A_CORET)
    return m


def chronomancer():
    m = []
    yb = 0.85
    caster_base(m, yb, A_SHELL, A_SHELL2, A_GOLD, eyes=SAND)
    # crown of small spikes around the hood.
    for px, py, pz in ring(5, 0, 0, 0.26, yb + 1.9):
        pyramid(m, px, pz, 0.06, py, py + 0.28, A_GOLD, n=4)
    # floating hourglass: two glowing frustums pinched at the waist, gold caps.
    hx, hz = 0.0, 0.72
    hy = yb + 0.85
    prism(m, hx, hz, 0.24, hy - 0.06, hy, A_GOLD, n=6)
    frustum(m, hx, hz, 0.19, 0.05, hy, hy + 0.24, SAND, n=6, emissive=True)
    frustum(m, hx, hz, 0.05, 0.19, hy + 0.24, hy + 0.48, SAND, n=6, emissive=True)
    prism(m, hx, hz, 0.24, hy + 0.48, hy + 0.54, A_GOLD, n=6)
    # the clock ring: twelve gold marks orbiting the body.
    for px, py, pz in ring(12, 0, 0, 0.95, yb + 0.85):
        mote(m, px, py, pz, 0.05, A_GOLD)
    return m


def seer():
    m = []
    # a floating eye familiar: no body, all gaze.
    y0 = 1.45
    frustum(m, 0, 0, 0.3, 0.56, y0, y0 + 0.38, A_SHELL, n=8)
    frustum(m, 0, 0, 0.56, 0.3, y0 + 0.38, y0 + 0.76, A_SHELL2, n=8)
    # iris and pupil on the front face.
    box(m, (-0.26, y0 + 0.18, 0.5), (0.26, y0 + 0.6, 0.62), A_TEAL, emissive=True)
    box(m, (-0.1, y0 + 0.28, 0.58), (0.1, y0 + 0.5, 0.68), A_VIO, emissive=True)
    # gold brow ring and lash spikes.
    prism(m, 0, 0, 0.34, y0 + 0.76, y0 + 0.82, A_GOLD, n=8)
    for px, py, pz in ring(4, 0, 0, 0.26, y0 + 0.82, rot=math.pi / 4):
        pyramid(m, px, pz, 0.06, py, py + 0.3, A_GOLD, n=4)
    # drifting scry-motes beneath.
    for i, t in enumerate((0.0, 0.4, 0.8)):
        mote(m, 0.3 - 0.5 * t, y0 - 0.25 - 0.45 * t, 0.2 * t, 0.055, A_CORET)
    return m


def wisp():
    m = []
    # a tiny darting familiar: glowing shard in a stone halo.
    y0 = 1.5
    crystal(m, 0, 0, 0.17, y0, y0 + 0.42, y0 + 0.8, A_TEAL, A_CORET, n=5)
    prism(m, 0, 0, 0.08, y0 + 0.05, y0 + 0.6, A_CORET, n=5, emissive=True)
    prism(m, 0, 0, 0.34, y0 + 0.18, y0 + 0.26, A_SHELL2, n=8)
    # motion trail.
    for i, t in enumerate((0.0, 0.35, 0.7, 1.0)):
        mote(m, -0.35 - 0.5 * t, y0 + 0.25 - 0.18 * t, -0.3 - 0.4 * t, 0.07 - 0.04 * t, A_CORET)
    return m


def tempest_dais():
    m = []
    # a flying stone disc crewed by witches, storm boiling beneath.
    yd = 2.05
    frustum(m, 0, 0, 0.55, 1.45, yd - 0.4, yd, A_SHELL2, n=8, top=False)
    frustum(m, 0, 0, 1.45, 1.6, yd, yd + 0.28, A_SHELL, n=8)
    for px, py, pz in ring(8, 0, 0, 1.42, yd + 0.28):
        box(m, (px - 0.07, py, pz - 0.07), (px + 0.07, py + 0.3, pz + 0.07), A_GOLD)
    # central storm focus.
    crystal(m, 0, 0, 0.2, yd + 0.28, yd + 0.75, yd + 1.15, A_SHELL2, STORM, n=5)
    prism(m, 0, 0, 0.1, yd + 0.34, yd + 1.0, BOLTC, n=5, emissive=True)
    # two crew witches at the rail.
    for cx, cz in ((-0.8, 0.35), (0.7, -0.45)):
        frustum(m, cx, cz, 0.24, 0.16, yd + 0.28, yd + 0.85, WITCH_ROBE, n=6)
        frustum(m, cx, cz, 0.3, 0.26, yd + 0.85, yd + 0.92, WITCH_ROBE2, n=6)
        pyramid(m, cx, cz, 0.16, yd + 0.92, yd + 1.25, WITCH_ROBE2, n=6)
    # the storm it rides on.
    cloud(m, 0.1, 0.0, yd - 1.05, scale=1.8)
    bolt(m, 0.35, 0.35, 0.0, yd - 1.05, BOLTC, seg=5, amp=0.24, s=0.07)
    return m


# ------------------------------------------------------------ Astro buildings


def athenaeum():
    m = []
    yb = 0.45  # hovers
    # floating foundation slab and plaza.
    frustum(m, 0, 0, 1.6, 2.3, yb, yb + 0.5, A_SHELL2, n=8, top=False)
    prism(m, 0, 0, 2.3, yb + 0.5, yb + 0.68, A_SHELL, n=8)
    # central lecture dome with a glowing scriptorium band.
    frustum(m, 0, 0, 1.25, 0.95, yb + 0.68, yb + 1.5, A_SHELL, n=8)
    prism(m, 0, 0, 1.27, yb + 0.95, yb + 1.18, A_VIO, n=8, emissive=True)
    frustum(m, 0, 0, 0.95, 0.45, yb + 1.5, yb + 2.15, A_SHELL2, n=8)
    pyramid(m, 0, 0, 0.45, yb + 2.15, yb + 3.0, A_GOLD, n=8)
    mote(m, 0, yb + 3.05, 0, 0.07, A_CORET)
    # three school spires around the dome.
    for px, _, pz in ring(3, 0, 0, 1.72, 0, rot=math.pi / 2):
        prism(m, px, pz, 0.24, yb + 0.68, yb + 2.3, A_SHELL2, n=6)
        prism(m, px, pz, 0.26, yb + 1.5, yb + 1.7, A_TEAL, n=6, emissive=True)
        pyramid(m, px, pz, 0.26, yb + 2.3, yb + 2.95, A_GOLD, n=6)
    # a ring of orbiting rune-tomes.
    for k, (px, py, pz) in enumerate(ring(5, 0, 0, 2.05, yb + 1.9)):
        mote(m, px, py + (0.12 if k % 2 else -0.08), pz, 0.09, A_COREV)
    return m


def storm_ward():
    m = []
    yb = 0.35  # hovers
    frustum(m, 0, 0, 0.95, 0.62, yb, yb + 0.4, A_SHELL2, n=6)
    frustum(m, 0, 0, 0.58, 0.32, yb + 0.4, yb + 2.3, A_SHELL, n=6)
    # charged rings up the obelisk.
    prism(m, 0, 0, 0.6, yb + 0.95, yb + 1.1, STORM, n=6, emissive=True)
    prism(m, 0, 0, 0.5, yb + 1.65, yb + 1.78, STORM, n=6, emissive=True)
    # the storm coil: alternating dark and charged discs, crowned by an orb.
    for k in range(3):
        y = yb + 2.3 + k * 0.3
        prism(m, 0, 0, 0.44, y, y + 0.14, (70, 78, 96), n=8)
        prism(m, 0, 0, 0.36, y + 0.14, y + 0.3, STORM, n=8, emissive=True)
    prism(m, 0, 0, 0.18, yb + 3.2, yb + 3.5, BOLTC, n=6, emissive=True)
    # test arcs off the crown.
    bolt(m, 0.5, 0.0, yb + 2.6, yb + 3.45, BOLTC, seg=3, amp=0.12)
    bolt(m, -0.45, 0.25, yb + 2.75, yb + 3.4, BOLTC, seg=3, amp=0.1)
    return m


# ----------------------------------------------------------- Hollowmen mechs


def mech_legs(m, spread, thigh_h, shin_h, w=0.26):
    """Simple two-post digitigrade legs with boots."""
    for sx in (-1, 1):
        x0 = sx * spread - w / 2
        box(m, (x0, shin_h, -0.3), (x0 + w, shin_h + thigh_h, 0.22), H_STEEL)
        box(m, (x0 + 0.02, 0.1, -0.16), (x0 + w - 0.02, shin_h + 0.06, 0.3), H_DARK)
        box(m, (x0 - 0.02, 0.0, -0.2), (x0 + w + 0.02, 0.12, 0.42), H_GUN)


def hound():
    m = []
    mech_legs(m, 0.34, 0.5, 0.45, w=0.2)
    # compact recon hull with a sensor visor.
    box(m, (-0.5, 0.95, -0.5), (0.5, 1.4, 0.45), H_STEEL)
    box(m, (-0.34, 1.05, 0.45), (0.34, 1.25, 0.52), H_CYAN, emissive=True)
    box(m, (-0.5, 1.12, -0.52), (0.5, 1.26, -0.48), H_HAZ)
    # the radar dish that makes it a walking detector.
    box(m, (-0.1, 1.4, -0.2), (0.1, 1.62, 0.0), H_DARK)
    frustum(m, 0, -0.1, 0.14, 0.52, 1.62, 1.88, H_STEEL2, n=8, top=False)
    prism(m, 0, -0.1, 0.1, 1.62, 1.95, H_CYAN, n=6, emissive=True)
    # whip antenna.
    box(m, (0.36, 1.4, -0.4), (0.42, 2.5, -0.34), H_GUN)
    mote(m, 0.39, 2.56, -0.37, 0.06, H_AMBER)
    return m


def javelin():
    m = []
    mech_legs(m, 0.4, 0.6, 0.5)
    box(m, (-0.4, 1.1, -0.3), (0.4, 1.35, 0.3), H_DARK)  # waist
    box(m, (-0.55, 1.35, -0.4), (0.55, 2.1, 0.38), H_STEEL)
    box(m, (-0.55, 1.5, 0.38), (0.55, 1.66, 0.42), H_HAZ)
    # sensor head.
    box(m, (-0.18, 2.1, -0.12), (0.18, 2.38, 0.22), H_STEEL2)
    box(m, (-0.12, 2.18, 0.22), (0.12, 2.3, 0.28), H_RED, emissive=True)
    # shoulder missile pods, cells glowing ready.
    for sx in (-1, 1):
        x0 = sx * 0.58
        x1 = sx * 1.08
        box(m, (min(x0, x1), 1.7, -0.42), (max(x0, x1), 2.3, 0.4), H_DARK)
        for r in range(2):
            for c in range(3):
                cx = min(x0, x1) + 0.1 + c * 0.14
                cy = 1.86 + r * 0.26
                box(m, (cx, cy, 0.4), (cx + 0.1, cy + 0.16, 0.46), H_AMBER, emissive=True)
    # one javelin away: missile and its exhaust trail.
    box(m, (0.72, 2.75, 0.5), (0.86, 2.95, 1.05), H_STEEL2)
    pyramid(m, 0.79, 1.05, 0.09, 2.85, 3.15, H_RED, n=4)
    for i, t in enumerate((0.0, 0.4, 0.8)):
        mote(m, 0.79, 2.6 - 0.45 * t, 0.42 - 0.3 * t, 0.07 - 0.03 * t, H_AMBER)
    return m


def wrecker():
    m = []
    mech_legs(m, 0.42, 0.55, 0.45)
    # hunched hull: lower block, upper block thrown forward.
    box(m, (-0.5, 1.0, -0.4), (0.5, 1.45, 0.25), H_STEEL)
    box(m, (-0.55, 1.45, -0.2), (0.55, 1.95, 0.6), H_STEEL2)
    box(m, (-0.55, 1.58, 0.6), (0.55, 1.72, 0.64), H_HAZ)
    # low-set head glaring out from under the cowl.
    box(m, (-0.16, 1.5, 0.6), (0.16, 1.72, 0.8), H_DARK)
    box(m, (-0.12, 1.56, 0.8), (0.12, 1.66, 0.84), H_RED, emissive=True)
    # massive breaching forearms ending in claws.
    for sx in (-1, 1):
        x0 = sx * 0.55
        x1 = sx * 0.95
        box(m, (min(x0, x1), 1.4, -0.1), (max(x0, x1), 1.9, 0.3), H_STEEL)
        box(m, (min(x0, x1) + 0.02, 0.45, 0.1), (max(x0, x1) - 0.02, 1.5, 0.62), H_DARK)
        for k in range(3):
            cx = min(x0, x1) + 0.07 + k * 0.13
            pyramid(m, cx, 0.66, 0.09, 0.5, 0.0, H_STEEL2, n=4)
    # the thermal lance glowing between the right claws.
    box(m, (0.66, 0.55, 0.62), (0.82, 0.7, 1.0), H_AMBER, emissive=True)
    mote(m, 0.74, 0.62, 1.08, 0.07, FIRE_CORE)
    return m


def bulwark():
    m = []
    mech_legs(m, 0.48, 0.55, 0.5, w=0.32)
    # wide, squat hull.
    box(m, (-0.75, 1.05, -0.45), (0.75, 1.75, 0.3), H_STEEL)
    box(m, (-0.75, 1.2, -0.47), (0.75, 1.36, -0.43), H_HAZ)
    box(m, (-0.2, 1.75, -0.2), (0.2, 1.95, 0.1), H_STEEL2)  # vision block
    box(m, (-0.14, 1.8, 0.1), (0.14, 1.9, 0.16), H_CYAN, emissive=True)
    # carrier arms thrust forward to the barrier.
    for sx in (-1, 1):
        box(m, (sx * 0.55 - 0.12, 1.3, 0.3), (sx * 0.55 + 0.12, 1.5, 0.78), H_DARK)
    # the barrier itself: a wall of light in a steel frame.
    box(m, (-1.15, 0.2, 0.78), (1.15, 2.0, 0.86), (150, 222, 244), emissive=True)
    box(m, (-1.2, 2.0, 0.74), (1.2, 2.12, 0.9), H_DARK)
    box(m, (-1.2, 0.08, 0.74), (1.2, 0.2, 0.9), H_DARK)
    # point-defense stubs on the shoulders.
    for sx in (-1, 1):
        box(m, (sx * 0.5 - 0.1, 1.75, -0.3), (sx * 0.5 + 0.1, 1.95, -0.1), H_GUN)
        box(m, (sx * 0.5 - 0.03, 1.95, -0.26), (sx * 0.5 + 0.03, 2.25, -0.2), H_GUN)
        mote(m, sx * 0.5, 2.3, -0.23, 0.045, H_AMBER)
    return m


# ----------------------------------------------------------- Hollowmen tanks


def tank_tracks(m, hw, hl, h=0.42):
    for sx in (-1, 1):
        x0 = sx * hw
        x1 = sx * (hw + 0.3)
        box(m, (min(x0, x1), 0.0, -hl), (max(x0, x1), h, hl), H_GUN)
        box(m, (min(x0, x1) - 0.02, h, -hl - 0.02), (max(x0, x1) + 0.02, h + 0.08, hl + 0.02),
            H_DARK)


def earthshaker():
    m = []
    tank_tracks(m, 0.7, 1.15)
    box(m, (-0.7, 0.3, -1.1), (0.7, 0.78, 1.1), H_STEEL)
    box(m, (-0.7, 0.46, 1.1), (0.7, 0.62, 1.14), H_HAZ)
    # rear casemate turret.
    box(m, (-0.48, 0.78, -1.0), (0.48, 1.3, -0.1), H_STEEL2)
    box(m, (-0.2, 1.3, -0.85), (0.2, 1.46, -0.45), H_DARK)  # hatch
    # the long gun, stepped up to its firing angle.
    for k in range(8):
        s = 0.14 - 0.008 * k
        y0 = 1.0 + 0.19 * k
        z0 = -0.35 + 0.33 * k
        box(m, (-s, y0, z0), (s, y0 + 0.22, z0 + 0.45), H_GUN)
    mote(m, 0.0, 2.55, 2.5, 0.09, H_AMBER)  # muzzle lamp
    # deployed recoil spades biting the ground at the rear.
    for sx in (-1, 1):
        pyramid(m, sx * 0.55, -1.32, 0.16, 0.7, 0.0, H_STEEL2, n=4)
    return m


def hailstorm():
    m = []
    tank_tracks(m, 0.62, 0.95)
    box(m, (-0.62, 0.3, -0.9), (0.62, 0.72, 0.9), H_STEEL)
    box(m, (-0.62, 0.42, 0.9), (0.62, 0.56, 0.94), H_HAZ)
    # AA turret with four stepped flak barrels.
    box(m, (-0.45, 0.72, -0.5), (0.45, 1.15, 0.35), H_STEEL2)
    for sx in (-0.3, -0.1, 0.1, 0.3):
        for k in range(4):
            y0 = 1.05 + 0.24 * k
            z0 = 0.15 + 0.2 * k
            box(m, (sx - 0.04, y0, z0), (sx + 0.04, y0 + 0.2, z0 + 0.3), H_GUN)
        mote(m, sx, 2.12, 1.18, 0.05, H_AMBER)
    # tracking dish on the rear deck.
    frustum(m, 0, -0.65, 0.08, 0.3, 1.15, 1.32, H_STEEL2, n=8, top=False)
    prism(m, 0, -0.65, 0.06, 1.15, 1.38, H_CYAN, n=6, emissive=True)
    # flak bursts overhead.
    for bx, by, bz in ((0.7, 3.0, 1.3), (-0.4, 3.3, 0.9), (0.2, 2.8, 1.7)):
        prism(m, bx, bz, 0.16, by, by + 0.2, CLOUD, n=6)
        mote(m, bx, by + 0.1, bz, 0.06, H_AMBER)
    return m


# ------------------------------------------------------------- Hollowmen air


def interceptor():
    m = []
    alt = 2.1
    # fuselage with a stepped nose taper.
    box(m, (-0.22, alt, -1.3), (0.22, alt + 0.42, 1.1), H_STEEL)
    box(m, (-0.16, alt + 0.04, 1.1), (0.16, alt + 0.34, 1.62), H_STEEL2)
    box(m, (-0.09, alt + 0.08, 1.62), (0.09, alt + 0.28, 2.0), H_STEEL2)
    box(m, (-0.12, alt + 0.42, 0.05), (0.12, alt + 0.6, 0.55), H_CYAN, emissive=True)  # canopy
    # swept wings, two steps back per side.
    for sx in (-1, 1):
        x0, x1 = sx * 0.22, sx * 0.95
        box(m, (min(x0, x1), alt + 0.14, -0.55), (max(x0, x1), alt + 0.24, 0.15), H_STEEL)
        x0, x1 = sx * 0.95, sx * 1.5
        box(m, (min(x0, x1), alt + 0.14, -0.85), (max(x0, x1), alt + 0.24, -0.3), H_STEEL2)
        mote(m, sx * 1.48, alt + 0.19, -0.32, 0.04, H_RED)  # wingtip light
    # tail.
    box(m, (-0.04, alt + 0.42, -1.28), (0.04, alt + 0.95, -0.85), H_STEEL2)
    for sx in (-1, 1):
        box(m, (min(sx * 0.2, sx * 0.6), alt + 0.34, -1.25),
            (max(sx * 0.2, sx * 0.6), alt + 0.42, -0.9), H_STEEL2)
    # afterburner.
    box(m, (-0.14, alt + 0.08, -1.48), (0.14, alt + 0.34, -1.3), H_AMBER, emissive=True)
    for i, t in enumerate((0.0, 0.4, 0.8)):
        mote(m, 0, alt + 0.2, -1.6 - 0.4 * t, 0.07 - 0.03 * t, H_AMBER)
    return m


def vulture():
    m = []
    alt = 2.3
    # heavy fuselage.
    box(m, (-0.35, alt, -1.5), (0.35, alt + 0.6, 1.3), H_STEEL)
    box(m, (-0.26, alt + 0.06, 1.3), (0.26, alt + 0.5, 1.75), H_STEEL2)
    box(m, (-0.16, alt + 0.34, 1.55), (0.16, alt + 0.5, 1.72), H_CYAN, emissive=True)  # cockpit
    box(m, (-0.35, alt + 0.24, -0.2), (0.35, alt + 0.34, -0.16), H_HAZ)
    # broad straight wings with twin nacelles each.
    for sx in (-1, 1):
        x0, x1 = sx * 0.35, sx * 1.9
        box(m, (min(x0, x1), alt + 0.34, -0.35), (max(x0, x1), alt + 0.46, 0.45), H_STEEL)
        for nx in (0.8, 1.45):
            box(m, (sx * nx - 0.1, alt + 0.12, -0.3), (sx * nx + 0.1, alt + 0.34, 0.5), H_DARK)
            box(m, (sx * nx - 0.07, alt + 0.16, -0.42), (sx * nx + 0.07, alt + 0.3, -0.3),
                H_AMBER, emissive=True)
    # tail plane and fin.
    box(m, (-0.05, alt + 0.6, -1.48), (0.05, alt + 1.2, -1.0), H_STEEL2)
    box(m, (-0.7, alt + 0.52, -1.45), (0.7, alt + 0.62, -1.05), H_STEEL2)
    # bomb bay open, stick falling.
    box(m, (-0.22, alt - 0.06, -0.5), (0.22, alt + 0.02, 0.4), H_GUN)
    for k, drop in enumerate((0.45, 1.05)):
        by = alt - drop
        bz = -0.1 - 0.25 * k
        box(m, (-0.07, by, bz - 0.2), (0.07, by + 0.18, bz + 0.2), H_GUN)
        pyramid(m, 0, bz - 0.24, 0.06, by + 0.09, by + 0.02, H_STEEL2, n=4)
    return m


# ------------------------------------------------------- Hollowmen buildings


def arsenal():
    m = []
    # main hall with a sawtooth roof.
    box(m, (-1.7, 0.0, -1.1), (0.5, 1.2, 1.1), H_STEEL)
    for k in range(3):
        x0 = -1.7 + k * 0.733
        box(m, (x0, 1.2, -1.1), (x0 + 0.5, 1.55, 1.1), H_STEEL2)
        box(m, (x0 + 0.5, 1.2, -1.05), (x0 + 0.72, 1.5, 1.05), H_CYAN, emissive=True)
    box(m, (-1.7, 0.45, 1.1), (0.5, 0.62, 1.14), H_HAZ)
    box(m, (-0.9, 0.0, 1.1), (-0.3, 0.8, 1.16), H_DARK)  # loading door
    box(m, (-0.82, 0.06, 1.16), (-0.38, 0.7, 1.18), H_AMBER, emissive=True)
    # annex with a raised missile rack.
    box(m, (0.5, 0.0, -0.85), (1.6, 0.85, 0.85), H_DARK)
    for k in range(3):
        y0 = 0.85 + 0.22 * k
        z0 = -0.55 + 0.16 * k
        box(m, (0.7, y0, z0), (1.4, y0 + 0.14, z0 + 1.0), H_GUN)
        box(m, (0.78, y0 + 0.02, z0 + 1.0), (0.9, y0 + 0.12, z0 + 1.2), H_RED, emissive=True)
        box(m, (1.2, y0 + 0.02, z0 + 1.0), (1.32, y0 + 0.12, z0 + 1.2), H_RED, emissive=True)
    # stack with rising smoke, and yard crates.
    prism(m, -1.35, -0.7, 0.2, 1.2, 2.4, CONCRETE2, n=8)
    for k, (dy, r) in enumerate(((0.15, 0.22), (0.5, 0.28), (0.9, 0.34))):
        prism(m, -1.35 + 0.1 * k, -0.7 - 0.08 * k, r, 2.4 + dy, 2.4 + dy + 0.22, CLOUD, n=6)
    box(m, (1.0, 0.0, -1.4), (1.5, 0.42, -0.95), H_HAZ)
    box(m, (0.45, 0.0, -1.5), (0.92, 0.36, -1.1), H_STEEL2)
    return m


def bunker():
    m = []
    # low cast-concrete blockhouse.
    frustum(m, 0, 0, 1.7, 1.45, 0.0, 0.3, CONCRETE2, n=4, rot=math.pi / 4)
    box(m, (-1.1, 0.25, -0.9), (1.1, 0.85, 0.9), CONCRETE)
    box(m, (-1.2, 0.85, -1.0), (1.2, 1.08, 1.0), CONCRETE2)  # roof slab
    # firing slit, manned and lit.
    box(m, (-0.72, 0.5, 0.9), (0.72, 0.68, 0.95), H_GUN)
    box(m, (-0.66, 0.53, 0.94), (0.66, 0.65, 0.97), H_AMBER, emissive=True)
    box(m, (-0.06, 0.52, 0.95), (0.06, 0.64, 1.5), H_GUN)  # the gun poking out
    # sandbag apron.
    for k in range(5):
        bx = -1.0 + k * 0.5
        box(m, (bx, 0.0, 1.06), (bx + 0.44, 0.26, 1.4), (120, 112, 88))
        if k < 4:
            box(m, (bx + 0.25, 0.26, 1.1), (bx + 0.69, 0.5, 1.36), (134, 124, 96))
    # comms antenna.
    box(m, (-1.05, 1.08, -0.85), (-0.99, 2.0, -0.79), H_GUN)
    mote(m, -1.02, 2.06, -0.82, 0.05, H_RED)
    return m


def flak_tower():
    m = []
    box(m, (-0.9, 0.0, -0.9), (0.9, 0.45, 0.9), CONCRETE2)
    box(m, (-0.55, 0.45, -0.55), (0.55, 2.1, 0.55), CONCRETE)
    box(m, (-0.55, 0.85, 0.55), (0.55, 1.0, 0.57), H_HAZ)
    # gun platform.
    box(m, (-0.8, 2.1, -0.8), (0.8, 2.4, 0.8), H_DARK)
    box(m, (-0.35, 2.4, -0.35), (0.35, 2.8, 0.35), H_STEEL2)
    # quad mount, barrels stepped skyward.
    for sx in (-0.22, 0.22):
        for sz in (-0.18, 0.18):
            for k in range(4):
                y0 = 2.7 + 0.27 * k
                z0 = sz + 0.13 * k
                box(m, (sx - 0.045, y0, z0 - 0.045), (sx + 0.045, y0 + 0.26, z0 + 0.1), H_GUN)
            mote(m, sx, 3.85, sz + 0.52, 0.05, H_AMBER)
    # spotting dish on the platform corner.
    frustum(m, -0.6, -0.6, 0.06, 0.26, 2.4, 2.56, H_STEEL2, n=8, top=False)
    prism(m, -0.6, -0.6, 0.05, 2.4, 2.62, H_CYAN, n=6, emissive=True)
    # flak bursts up high.
    for bx, by, bz in ((0.9, 4.4, 0.8), (-0.6, 4.7, 0.4), (0.2, 4.2, 1.2)):
        prism(m, bx, bz, 0.15, by, by + 0.18, CLOUD, n=6)
        mote(m, bx, by + 0.09, bz, 0.055, H_AMBER)
    return m


# ----------------------------------------------- tech tree buildings, astro


def crucible():
    m = []
    yb = 0.4  # hovers
    # grown forge: a wide bowl of living stone over a molten heart.
    frustum(m, 0, 0, 1.3, 1.9, yb, yb + 0.55, A_SHELL2, n=8, top=False)
    frustum(m, 0, 0, 1.9, 1.5, yb + 0.55, yb + 1.15, A_SHELL, n=8, top=False)
    prism(m, 0, 0, 1.42, yb + 1.0, yb + 1.1, A_SHELL2, n=8)  # bowl lip
    prism(m, 0, 0, 1.05, yb + 1.02, yb + 1.14, EMBER, n=8, emissive=True)  # melt pool
    prism(m, 0, 0, 0.55, yb + 1.04, yb + 1.22, FIRE_CORE, n=8, emissive=True)
    prism(m, 0, 0, 1.92, yb + 0.5, yb + 0.62, A_GOLD, n=8)  # ceremonial band
    # three horn spires around the rim, channeling the heat.
    for px, _, pz in ring(3, 0, 0, 1.75, 0, rot=math.pi / 6):
        crystal(m, px, pz, 0.24, yb + 0.9, yb + 1.9, yb + 2.5, A_SHELL2, EMBER, n=5)
    # embers rising off the melt.
    for i, t in enumerate((0.0, 0.45, 0.9)):
        mote(m, 0.3 - 0.25 * t, yb + 1.3 + 0.65 * t, -0.2 + 0.3 * t, 0.06, FLAME)
    return m


def conservatory():
    m = []
    yb = 0.4  # hovers
    # tiered scroll-dome: stacked discs with glowing seams, crowned in crystal.
    frustum(m, 0, 0, 1.5, 1.8, yb, yb + 0.4, A_SHELL2, n=8, top=False)
    for k, (r, h) in enumerate(((1.5, 0.5), (1.15, 0.45), (0.8, 0.4))):
        y0 = yb + 0.4 + sum((0.5, 0.45, 0.4)[:k]) + 0.12 * k
        prism(m, 0, 0, r, y0, y0 + h, A_SHELL, n=8)
        prism(m, 0, 0, r * 0.86, y0 + h, y0 + h + 0.12, A_TEAL, n=8, emissive=True)
    crystal(m, 0, 0, 0.3, yb + 2.1, yb + 2.7, yb + 3.3, A_VIO, A_COREV, n=6)
    # two flanking lectern pylons with orbiting runes.
    for sx in (-1, 1):
        prism(m, sx * 1.55, 0.0, 0.2, yb, yb + 1.5, A_SHELL2, n=6)
        pyramid(m, sx * 1.55, 0.0, 0.2, yb + 1.5, yb + 1.95, A_GOLD, n=6)
        mote(m, sx * 1.55, yb + 2.2, 0.0, 0.08, A_COREV)
    return m


def aerie():
    m = []
    yb = 0.4  # hovers
    # a roost spire with two cantilevered perch discs.
    frustum(m, 0, 0, 1.3, 0.9, yb, yb + 0.5, A_SHELL2, n=8)
    frustum(m, 0, 0, 0.65, 0.3, yb + 0.5, yb + 3.2, A_SHELL, n=6)
    pyramid(m, 0, 0, 0.34, yb + 3.2, yb + 3.9, A_GOLD, n=6)
    mote(m, 0, yb + 3.95, 0, 0.08, A_CORET)  # roost beacon
    for sx, py in ((-1, 1.1), (1, 2.2)):
        cx = sx * 1.25
        box(m, (min(0.0, cx), yb + py - 0.14, -0.18), (max(0.0, cx), yb + py, 0.18), A_SHELL2)
        prism(m, cx, 0.0, 0.55, yb + py - 0.1, yb + py + 0.04, A_SHELL, n=8)
        prism(m, cx, 0.0, 0.42, yb + py + 0.04, yb + py + 0.1, A_TEAL, n=8, emissive=True)
    # a hover-craft on approach above the high perch.
    hx, hy = 1.45, yb + 3.3
    prism(m, hx, 0.0, 0.3, hy, hy + 0.26, A_SHELL2, n=6)
    box(m, (hx - 0.62, hy + 0.08, -0.12), (hx + 0.62, hy + 0.18, 0.12), A_SHELL)
    mote(m, hx, hy - 0.14, 0.0, 0.06, A_CORET)
    return m


def ley_nexus():
    m = []
    yb = 0.35  # hovers
    # terraced mana well: stacked rings around a font, a great crystal above.
    frustum(m, 0, 0, 2.2, 1.8, yb, yb + 0.4, A_SHELL2, n=8)
    frustum(m, 0, 0, 1.6, 1.25, yb + 0.4, yb + 0.8, A_SHELL, n=8)
    prism(m, 0, 0, 1.05, yb + 0.8, yb + 0.94, A_TEAL, n=8, emissive=True)  # ley pool
    frustum(m, 0, 0, 0.5, 0.3, yb + 0.94, yb + 1.7, A_SHELL2, n=6)  # font column
    # the beam feeding the suspended heart-crystal.
    box(m, (-0.07, yb + 1.7, -0.07), (0.07, yb + 2.5, 0.07), A_CORET, emissive=True)
    crystal(m, 0, 0, 0.42, yb + 2.5, yb + 3.2, yb + 3.9, A_VIO, A_COREV, n=6)
    prism(m, 0, 0, 0.2, yb + 2.6, yb + 3.3, A_COREV, n=6, emissive=True)
    # gold votive stones and drifting mana on the terrace.
    for k, (px, py, pz) in enumerate(ring(5, 0, 0, 1.45, yb + 0.94)):
        box(m, (px - 0.1, py, pz - 0.1), (px + 0.1, py + 0.3 + 0.08 * (k % 2), pz + 0.1), A_GOLD)
    for i, t in enumerate((0.0, 0.5, 1.0)):
        mote(m, 1.0 - 0.3 * t, yb + 1.3 + 0.8 * t, -0.9 + 0.45 * t, 0.06, A_COREV)
    return m


# -------------------------------------------- tech tree buildings, hollowmen


def machine_shop():
    m = []
    # the factory add-on: a gabled annex with a crane and a spare gear.
    box(m, (-1.1, 0.0, -0.8), (0.7, 1.0, 0.8), H_STEEL)
    gable_y = 1.0
    box(m, (-1.15, gable_y, -0.85), (0.75, 1.12, 0.85), H_DARK)
    box(m, (-0.9, 1.12, -0.5), (0.1, 1.5, 0.5), H_STEEL2)  # roof house
    box(m, (-0.45, 0.0, 0.8), (0.35, 0.85, 0.86), H_DARK)  # shutter door
    box(m, (-0.38, 0.06, 0.86), (0.28, 0.75, 0.88), H_AMBER, emissive=True)
    box(m, (-1.1, 0.55, 0.8), (0.7, 0.7, 0.84), H_HAZ)
    # crane arm out over the yard.
    box(m, (0.7, 0.0, -0.2), (0.9, 1.7, 0.0), H_GUN)
    box(m, (0.6, 1.7, -0.18), (1.6, 1.85, -0.02), H_HAZ)
    box(m, (1.42, 1.1, -0.14), (1.5, 1.7, -0.06), H_GUN)  # hook cable
    # the spare gear leaned on the wall: a toothed disc.
    prism(m, 1.2, 0.55, 0.42, 0.0, 0.18, H_STEEL2, n=8)
    for px, _, pz in ring(8, 1.2, 0.55, 0.5, 0.0):
        box(m, (px - 0.06, 0.0, pz - 0.06), (px + 0.06, 0.16, pz + 0.06), H_STEEL2)
    prism(m, 1.2, 0.55, 0.12, 0.0, 0.2, H_DARK, n=8)
    return m


def radar_array():
    m = []
    # ops hut plus a lattice mast carrying the main dish.
    box(m, (-1.3, 0.0, -0.7), (-0.1, 0.8, 0.7), H_STEEL)
    box(m, (-1.3, 0.3, 0.7), (-0.1, 0.45, 0.74), H_HAZ)
    box(m, (-1.0, 0.8, -0.4), (-0.4, 1.0, 0.4), H_STEEL2)
    box(m, (-0.95, 0.25, 0.7), (-0.45, 0.6, 0.73), H_CYAN, emissive=True)  # ops window
    # the mast, narrowing in stages.
    box(m, (0.35, 0.0, -0.45), (1.15, 0.3, 0.45), CONCRETE2)
    box(m, (0.5, 0.3, -0.3), (1.0, 1.5, 0.3), H_DARK)
    box(m, (0.58, 1.5, -0.22), (0.92, 2.4, 0.22), H_GUN)
    # main dish opening skyward, glowing feed at its heart.
    frustum(m, 0.75, 0.0, 0.18, 0.85, 2.4, 2.85, H_STEEL2, n=8, top=False)
    prism(m, 0.75, 0.0, 0.1, 2.4, 3.0, H_CYAN, n=6, emissive=True)
    mote(m, 0.75, 3.1, 0.0, 0.06, H_CYAN)
    # small spotter dish on the hut roof.
    frustum(m, -0.7, 0.0, 0.06, 0.3, 1.0, 1.18, H_STEEL2, n=8, top=False)
    mote(m, -0.7, 1.26, 0.0, 0.045, H_RED)
    return m


def starport():
    m = []
    # apron slab with lit landing strips, tower at the corner, fuel farm behind.
    box(m, (-1.7, 0.0, -1.4), (1.5, 0.3, 1.4), CONCRETE2)
    box(m, (-1.0, 0.3, -1.0), (1.2, 0.38, 1.1), H_DARK)  # pad
    for sz in (-0.55, 0.05, 0.65):
        box(m, (-0.7, 0.38, sz), (0.9, 0.42, sz + 0.1), H_CYAN, emissive=True)
    # control tower.
    box(m, (-1.6, 0.3, -1.3), (-1.0, 1.9, -0.7), H_STEEL)
    box(m, (-1.7, 1.9, -1.4), (-0.9, 2.3, -0.6), H_STEEL2)
    box(m, (-1.66, 2.0, -0.62), (-0.94, 2.2, -0.58), H_CYAN, emissive=True)
    mote(m, -1.3, 2.42, -1.0, 0.05, H_RED)
    # fuel tanks on the back lot.
    for k, (tx, tz) in enumerate(((1.2, -1.0), (0.7, -1.15))):
        prism(m, tx, tz, 0.26, 0.3, 0.95 - 0.15 * k, H_STEEL2, n=8)
        prism(m, tx, tz, 0.27, 0.55, 0.64, H_HAZ, n=8)
    # a gunship flaring to land, running lights on.
    gx, gy = 0.15, 1.6
    box(m, (gx - 0.3, gy, -0.45), (gx + 0.3, gy + 0.34, 0.5), H_STEEL)
    box(m, (gx - 0.2, gy + 0.06, 0.5), (gx + 0.2, gy + 0.3, 0.72), H_CYAN, emissive=True)
    for sx in (-1, 1):
        box(m, (gx + sx * 0.3, gy + 0.2, -0.2), (gx + sx * 0.75, gy + 0.3, 0.15), H_STEEL2)
        mote(m, gx + sx * 0.72, gy + 0.1, 0.0, 0.05, H_AMBER)
    return m


def fusion_reactor():
    m = []
    # containment dome flanked by waisted cooling towers, all piped together.
    frustum(m, 0, 0, 1.35, 1.1, 0.0, 0.5, CONCRETE2, n=8)
    frustum(m, 0, 0, 1.1, 0.75, 0.5, 1.3, H_STEEL, n=8)
    frustum(m, 0, 0, 0.75, 0.3, 1.3, 1.8, H_STEEL2, n=8)
    prism(m, 0, 0, 1.12, 0.95, 1.12, H_CYAN, n=8, emissive=True)  # core seam
    prism(m, 0, 0, 0.16, 1.8, 2.1, H_CYAN, n=6, emissive=True)  # vent flare
    box(m, (-1.15, 0.2, -0.1), (1.15, 0.45, 0.1), H_GUN)  # main coolant trunk
    # the two cooling towers, narrow at the waist, steaming.
    for sx in (-1, 1):
        cx = sx * 1.55
        frustum(m, cx, 0.0, 0.62, 0.38, 0.0, 1.1, CONCRETE, n=8, top=False)
        frustum(m, cx, 0.0, 0.38, 0.52, 1.1, 1.9, CONCRETE, n=8, top=False)
        prism(m, cx, 0.0, 0.4, 1.9, 2.0, CONCRETE2, n=8)
        cloud(m, cx, 0.0, 2.25, scale=0.7)
        box(m, (min(cx, 0.0) + 0.2, 0.2, -0.08), (max(cx, 0.0) - 0.2, 0.4, 0.08), H_GUN)
    box(m, (-0.9, 0.0, 0.95), (-0.2, 0.55, 1.45), H_STEEL)  # switch house
    box(m, (-0.82, 0.1, 1.45), (-0.28, 0.42, 1.47), H_AMBER, emissive=True)
    return m


def drydock():
    m = []
    # gantry frame straddling a capital hull mid-assembly.
    for sz in (-1, 1):
        for sx in (-1, 1):
            box(m, (sx * 1.5 - 0.18, 0.0, sz * 1.0 - 0.18),
                (sx * 1.5 + 0.18, 2.3, sz * 1.0 + 0.18), H_DARK)
        box(m, (-1.68, 2.3, sz * 1.0 - 0.14), (1.68, 2.55, sz * 1.0 + 0.14), H_HAZ)
    box(m, (-0.4, 2.55, -1.1), (0.4, 2.8, 1.1), H_GUN)  # traveling crane
    box(m, (-0.06, 1.6, -0.3), (0.06, 2.55, -0.22), H_GUN)  # cable
    # the hull on blocks: plated aft, bare frames forward.
    for bx in (-0.9, 0.0, 0.9):
        box(m, (bx - 0.15, 0.0, -0.3), (bx + 0.15, 0.5, 0.3), CONCRETE2)
    box(m, (-1.3, 0.5, -0.55), (0.5, 1.3, 0.55), H_STEEL)
    box(m, (0.5, 0.55, -0.45), (1.0, 1.2, 0.45), H_DARK)  # unplated bow frames
    box(m, (1.0, 0.6, -0.3), (1.35, 1.1, 0.3), H_DARK)
    box(m, (-1.3, 0.85, 0.55), (-0.2, 1.0, 0.59), H_CYAN, emissive=True)  # lit ports
    # welding sparks where the plating ends.
    mote(m, 0.5, 1.25, 0.2, 0.07, (255, 246, 210))
    mote(m, 0.62, 1.05, -0.25, 0.05, H_AMBER)
    return m


def missile_silo():
    m = []
    # hardened apron, doors swung open, the nuke standing ready.
    box(m, (-1.4, 0.0, -1.4), (1.4, 0.5, 1.4), CONCRETE2)
    box(m, (-1.4, 0.18, 1.4), (1.4, 0.32, 1.44), H_HAZ)
    prism(m, 0, 0, 0.95, 0.5, 0.7, CONCRETE, n=8)  # silo collar
    prism(m, 0, 0, 0.72, 0.5, 0.72, H_GUN, n=8)  # the shaft mouth
    # blast doors laid open either side.
    for sx in (-1, 1):
        box(m, (sx * 1.0, 0.5, -0.75), (sx * 1.9, 0.66, 0.75), H_STEEL2)
        box(m, (sx * 1.0, 0.66, -0.75), (sx * 1.9, 0.7, -0.55), H_HAZ)
    # the missile, nosing out of the shaft.
    prism(m, 0, 0, 0.34, 0.6, 1.9, H_STEEL2, n=8)
    frustum(m, 0, 0, 0.34, 0.16, 1.9, 2.5, H_STEEL, n=8)
    pyramid(m, 0, 0, 0.17, 2.5, 3.0, H_RED, n=8)
    box(m, (-0.36, 1.0, -0.05), (0.36, 1.2, 0.05), H_HAZ)  # body band
    # klaxon lights around the collar.
    for px, _, pz in ring(4, 0, 0, 1.05, 0, rot=math.pi / 4):
        mote(m, px, 0.78, pz, 0.05, H_RED)
    # ops bunker in the corner.
    box(m, (0.7, 0.5, -1.3), (1.35, 0.95, -0.75), H_STEEL)
    box(m, (0.78, 0.58, -0.75), (1.27, 0.82, -0.73), H_CYAN, emissive=True)
    return m


# ------------------------------------------------------------------- sheets


HOVER = {"fit": 0.7, "ground": 1.5, "shadow": 1.3, "sa": 0.4}
GROUND = {"fit": 0.7, "ground": 1.5, "shadow": 1.5, "sa": 0.5}
VEHICLE = {"fit": 0.74, "ground": 1.9, "shadow": 1.9, "sa": 0.5}
FLYER = {"fit": 0.72, "ground": 1.3, "shadow": 1.4, "sa": 0.35}
BUILDING = {"fit": 0.78, "ground": 2.8, "shadow": 2.6, "sa": 0.5}


def render_sheet(path, title, subtitle, rows):
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
    H = margin + title_h + len(rows) * row_h + margin // 2

    cv = Canvas(W, H)
    gradient_bg(cv)
    text(cv, margin, margin, title, 3 * SS, (228, 234, 246))
    text(cv, margin, margin + 26 * SS, subtitle, 2 * SS, (120, 150, 190))

    y0 = margin + title_h
    for ri, (htext, hcol, accent, cells) in enumerate(rows):
        ry = y0 + ri * row_h
        cv.fill_rect(margin, ry, margin + grid_w, ry + head - 8 * SS, (18, 24, 38))
        cv.fill_rect(margin, ry, margin + 6 * SS, ry + head - 8 * SS, accent)
        text(cv, margin + 16 * SS, ry + 8 * SS, htext, 3 * SS, hcol)
        cy0 = ry + head
        for ci, (name, fn, opts) in enumerate(cells):
            x = margin + ci * (cell + gap)
            cv.fill_rect(x, cy0, x + cell, cy0 + cell, (13, 18, 30))
            for t in range(2 * SS):
                cv.fill_rect(x + t, cy0 + t, x + cell - t, cy0 + t + 1, (40, 52, 74))
                cv.fill_rect(x + t, cy0 + cell - t - 1, x + cell - t, cy0 + cell - t, (40, 52, 74))
                cv.fill_rect(x + t, cy0 + t, x + t + 1, cy0 + cell - t, (40, 52, 74))
                cv.fill_rect(x + cell - t - 1, cy0 + t, x + cell - t, cy0 + cell - t, (40, 52, 74))
            ccx = x + cell // 2
            ccy = cy0 + cell // 2 + 18 * SS
            render_object(cv, fn(), ccx, ccy, cell, cell, fit=opts["fit"],
                          ground_r=opts["ground"], shadow_r=opts["shadow"],
                          shadow_a=opts["sa"])
            lw = text_w(name, 2 * SS)
            text(cv, x + (cell - lw) // 2, cy0 + cell + 6 * SS, name, 2 * SS, (208, 216, 230))

    fw, fh, out = downsample(cv)
    write_png(path, fw, fh, out)
    print("wrote", path, fw, "x", fh)


def main():
    os.makedirs(OUT_DIR, exist_ok=True)
    vio = (176, 120, 240)
    teal = (150, 220, 200)
    amber = (230, 170, 80)
    red = (240, 130, 110)

    astro_rows = [
        ("BATTLE CASTERS  -  THE ATHENAEUM SCHOOLS", vio, A_VIO,
         [("PYROMANCER - FIRESTORM", pyromancer, HOVER),
          ("STORMCALLER - ANTI-AIR", stormcaller, HOVER),
          ("HEX-WITCH - CURSES", hex_witch, HOVER)]),
        ("SUPPORT CASTERS", vio, A_TEAL,
         [("DRUID - REGROWTH", druid, HOVER),
          ("EVOKER - CONSTRUCTS", evoker, HOVER),
          ("CHRONOMANCER - TIME", chronomancer, HOVER)]),
        ("EYES AND AIR", teal, A_CORET,
         [("SEER - DETECTOR", seer, HOVER),
          ("WISP - SCOUT", wisp, dict(HOVER, fit=0.62)),
          ("TEMPEST DAIS - FLYER", tempest_dais, dict(FLYER, fit=0.76, ground=1.8))]),
        ("STRUCTURES", vio, A_GOLD,
         [("ATHENAEUM - CASTER HALL", athenaeum, BUILDING),
          ("STORM-WARD - STATIC AA", storm_ward, dict(BUILDING, fit=0.7, ground=1.8))]),
    ]
    render_sheet(
        os.path.join(OUT_DIR, "roster_astro.png"),
        "ASTROMANCERS  -  ROSTER EXPANSION CONCEPTS",
        "DESIGN DRAFT  -  ROUGH MASSING / SILHOUETTE INTENT, NOT FINAL ART",
        astro_rows,
    )

    hollow_rows = [
        ("MECH FAMILY  -  CHASSIS VARIANTS", amber, H_HAZ,
         [("HOUND - RECON RADAR", hound, GROUND),
          ("JAVELIN - MISSILES", javelin, GROUND),
          ("WRECKER - MELEE", wrecker, GROUND)]),
        ("MECHS AND TANKS", amber, H_CYAN,
         [("BULWARK - BARRIER", bulwark, GROUND),
          ("EARTHSHAKER - ARTILLERY", earthshaker, VEHICLE),
          ("HAILSTORM - FLAK TANK", hailstorm, VEHICLE)]),
        ("AIR WING", red, H_RED,
         [("INTERCEPTOR - FIGHTER", interceptor, FLYER),
          ("VULTURE - HEAVY BOMBER", vulture, dict(FLYER, fit=0.76))]),
        ("STRUCTURES", amber, H_HAZ,
         [("ARSENAL - MUNITIONS", arsenal, BUILDING),
          ("BUNKER - GARRISON", bunker, dict(BUILDING, fit=0.72)),
          ("FLAK TOWER - STATIC AA", flak_tower, dict(BUILDING, fit=0.72))]),
    ]
    render_sheet(
        os.path.join(OUT_DIR, "roster_hollow.png"),
        "HOLLOWMEN  -  ROSTER EXPANSION CONCEPTS",
        "DESIGN DRAFT  -  ROUGH MASSING / SILHOUETTE INTENT, NOT FINAL ART",
        hollow_rows,
    )

    building_rows = [
        ("ASTROMANCER TECH  -  GROWN PRODUCTION", vio, A_VIO,
         [("CRUCIBLE - HEAVY GROUND", crucible, BUILDING),
          ("CONSERVATORY - RESEARCH", conservatory, BUILDING),
          ("AERIE - AIR ROOST", aerie, dict(BUILDING, fit=0.74))]),
        ("ASTROMANCER ENDGAME", vio, A_COREV,
         [("LEY NEXUS - SUPERWEAPON", ley_nexus, BUILDING)]),
        ("HOLLOWMEN TECH", amber, H_HAZ,
         [("MACHINE SHOP - ADD-ON", machine_shop, BUILDING),
          ("RADAR ARRAY - DETECTION", radar_array, BUILDING),
          ("STARPORT - AIR", starport, BUILDING)]),
        ("HOLLOWMEN ENDGAME", amber, H_RED,
         [("FUSION REACTOR - POWER", fusion_reactor, BUILDING),
          ("DRYDOCK - CAPITAL YARD", drydock, BUILDING),
          ("MISSILE SILO - NUKE", missile_silo, BUILDING)]),
    ]
    render_sheet(
        os.path.join(OUT_DIR, "roster_buildings.png"),
        "TECH TREE BUILDINGS  -  CONCEPT DRAFT",
        "COMPLETES BUILDINGS.PNG  -  ROUGH MASSING, NOT FINAL ART",
        building_rows,
    )


if __name__ == "__main__":
    main()
