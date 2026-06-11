#!/usr/bin/env python3
"""Generate chunky, hand-painted-ish terrain tiles as PNGs (no dependencies).

Run:  python3 assets/gen_textures.py
Outputs 64x64 tiles to assets/textures/ for the client to sample with linear
filtering (a soft painterly look). Edit the palettes below and re-run.
"""

import os
import struct
import zlib
import math

OUT = os.path.join(os.path.dirname(__file__), "textures")
SIZE = 64


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
        raw.append(0)  # filter: none
        raw += rgba[y * w * 4 : (y + 1) * w * 4]
    out = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )
    with open(path, "wb") as f:
        f.write(out)


# Small deterministic value-noise whose lattice wraps every `period` cells,
# so a tile sampled over [0, period) is seamless.
def vnoise(seed, period=8):
    g = {}

    def grad(ix, iy):
        key = (ix % period, iy % period)
        if key not in g:
            n = (key[0] * 1619 + key[1] * 31337 + seed * 1013) & 0x7FFFFFFF
            n = (n ^ (n >> 13)) * 1274126177 & 0x7FFFFFFF
            g[key] = (n % 1000) / 1000.0
        return g[key]

    def smooth(t):
        return t * t * (3 - 2 * t)

    def at(x, y):
        ix, iy = int(math.floor(x)), int(math.floor(y))
        fx, fy = x - ix, y - iy
        a = grad(ix, iy)
        b = grad(ix + 1, iy)
        c = grad(ix, iy + 1)
        d = grad(ix + 1, iy + 1)
        ux, uy = smooth(fx), smooth(fy)
        return (a + (b - a) * ux) + ((c + (d - c) * ux) - (a + (b - a) * ux)) * uy

    return at


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def rnd(x, y, s):
    """Deterministic per-pixel hash in [0, 1) - scattered, not periodic."""
    n = (x * 374761393 + y * 668265263 + s * 362437) & 0xFFFFFFFF
    n = ((n ^ (n >> 13)) * 1274126177) & 0xFFFFFFFF
    return ((n ^ (n >> 16)) & 0xFFFF) / 65535.0


def tile(name, lo, hi, scale, seed, light=None, dark=None, light_p=0.07, dark_p=0.06):
    # Three octaves on wrapping lattices: broad patches, mid clumps, and a
    # fine grain, each sampled over exactly one period so the tile is seamless.
    period = max(2, int(round(scale)))
    n = vnoise(seed, period)
    n2 = vnoise(seed + 7, period * 2)
    n3 = vnoise(seed + 13, period * 4)
    px = bytearray()
    for y in range(SIZE):
        for x in range(SIZE):
            u, v = x / SIZE * period, y / SIZE * period
            t = (
                n(u, v) * 0.52
                + n2(u * 2.0 + 4.0, v * 2.0 + 9.0) * 0.30
                + n3(u * 4.0 + 2.0, v * 4.0 + 5.0) * 0.18
            )
            t = max(0.0, min(1.0, t))
            c = lerp(lo, hi, t)
            # Randomly scattered light/dark flecks for a hand-painted look,
            # softened toward the base color so they read as brushed dabs.
            r = rnd(x, y, seed * 7 + 1)
            if light and r < light_p:
                c = lerp(c, light, 0.75)
            elif dark and r > 1.0 - dark_p:
                c = lerp(c, dark, 0.75)
            px += bytes((c[0], c[1], c[2], 255))
    write_png(os.path.join(OUT, name), SIZE, SIZE, px)


def detail_tile(name, seed=11, size=128):
    """The model surface-detail map the unit shader triplanar-maps over
    unit/building meshes. Two materials share the tile, one per channel:

      R - plate/masonry: panel grain, jittered seam lines, per-panel tone
          shifts, scratches, rivets and pitting (metal hulls, carved stone).
      G - organic grain: pure multi-octave noise with wandering crack veins,
          no straight lines (rock, dirt, scree).

    Both are authored around mid-grey = "no change"; the shader turns the
    selected channel into a brightness multiplier."""
    period = 8
    n = vnoise(seed, period)
    n2 = vnoise(seed + 7, period * 2)
    n3 = vnoise(seed + 13, period * 4)
    # The organic channel's own lattices, plus a smooth vein field whose
    # zero-crossings become wandering cracks.
    nr = vnoise(seed + 21, period)
    nr2 = vnoise(seed + 27, period * 2)
    nr3 = vnoise(seed + 33, period * 4)
    nv = vnoise(seed + 41, period)
    # Jittered, wrap-friendly panel seams on each axis.
    panels = 4
    seams_x = [
        (k * size // panels + int(rnd(k, 0, seed) * 12) - 6) % size for k in range(panels)
    ]
    seams_y = [
        (k * size // panels + int(rnd(0, k, seed + 1) * 12) - 6) % size for k in range(panels)
    ]

    def wrap_dist(a, seams):
        return min(min(abs(a - s), size - abs(a - s)) for s in seams)

    def panel_of(a, seams):
        return sum(1 for s in seams if a >= s)

    # Pre-painted scratches: short light streaks at random angles.
    scratch = {}
    for i in range(60):
        x = rnd(i, 3, seed * 5) * size
        y = rnd(i, 7, seed * 5) * size
        ang = rnd(i, 9, seed * 5) * 6.28318
        ln = 4 + rnd(i, 11, seed * 5) * 9.0
        for s in range(int(ln)):
            sx = int(x + math.cos(ang) * s) % size
            sy = int(y + math.sin(ang) * s) % size
            scratch[(sx, sy)] = 0.10 + rnd(i, 13, seed * 5) * 0.08
    # Rivets: bright dots inset from seam crossings.
    rivets = set()
    for sx in seams_x:
        for sy in seams_y:
            for ox, oy in ((4, 4), (-5, 4), (4, -5), (-5, -5)):
                rivets.add(((sx + ox) % size, (sy + oy) % size))

    px = bytearray()
    for y in range(size):
        for x in range(size):
            u, v = x / size * period, y / size * period
            t = (
                n(u, v) * 0.50
                + n2(u * 2.0 + 4.0, v * 2.0 + 9.0) * 0.30
                + n3(u * 4.0 + 2.0, v * 4.0 + 5.0) * 0.20
            )
            val = 0.5 + (t - 0.5) * 0.24
            # Per-panel tone shift so plates read as separate pieces.
            val += (rnd(panel_of(x, seams_x), panel_of(y, seams_y), seed * 3) - 0.5) * 0.09
            # Seam lines: a dark groove with a softer shoulder.
            dx, dy = wrap_dist(x, seams_x), wrap_dist(y, seams_y)
            d = min(dx, dy)
            if d == 0:
                val -= 0.18
            elif d == 1:
                val -= 0.08
            # Scratches, rivets, pitting.
            val += scratch.get((x, y), 0.0)
            if (x, y) in rivets or (x, y - 1) in rivets:
                val += 0.14
            if rnd(x, y, seed * 9) < 0.02:
                val -= 0.10
            plate = int(max(0.18, min(0.85, val)) * 255)
            # Organic channel: noise grain + crack veins + pitting only.
            tr = (
                nr(u, v) * 0.45
                + nr2(u * 2.0 + 3.0, v * 2.0 + 6.0) * 0.33
                + nr3(u * 4.0 + 1.0, v * 4.0 + 8.0) * 0.22
            )
            rv = 0.5 + (tr - 0.5) * 0.34
            vein = abs(nv(u + 2.5, v + 1.5) - 0.5) * 2.0
            if vein < 0.12:
                rv -= 0.10 * (1.0 - vein / 0.12)
            r2 = rnd(x, y, seed * 17)
            if r2 < 0.03:
                rv -= 0.09
            elif r2 > 0.985:
                rv += 0.08
            rock = int(max(0.18, min(0.85, rv)) * 255)
            px += bytes((plate, rock, 128, 255))
    write_png(os.path.join(OUT, name), size, size, px)


def main():
    os.makedirs(OUT, exist_ok=True)
    # WC3-ish warm, saturated, limited palettes with scattered flecks.
    tile("grass.png", (52, 88, 36), (92, 134, 56), 4.0, 1, light=(142, 178, 88), dark=(40, 66, 28))
    tile("dirt.png", (94, 70, 40), (136, 104, 62), 4.0, 2, light=(160, 126, 78), dark=(74, 52, 30))
    tile("rock.png", (76, 72, 68), (122, 116, 106), 5.0, 3, light=(150, 144, 134), dark=(52, 48, 44))
    tile("sand.png", (152, 136, 90), (198, 180, 122), 3.5, 4, light=(214, 198, 142), dark=(132, 116, 74))
    tile("water.png", (26, 68, 108), (44, 112, 152), 3.0, 5, light=(70, 140, 176), dark=(20, 52, 86))
    detail_tile("detail.png")
    print("wrote tiles to", OUT)


if __name__ == "__main__":
    main()
