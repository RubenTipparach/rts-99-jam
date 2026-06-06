#!/usr/bin/env python3
"""Generate chunky, hand-painted-ish terrain tiles as PNGs (no dependencies).

Run:  python3 assets/gen_textures.py
Outputs 32x32 tiles to assets/textures/ for the client to load with
nearest-neighbour sampling (PS1 / WC3 look). Edit the palettes below and re-run.
"""

import os
import struct
import zlib
import math

OUT = os.path.join(os.path.dirname(__file__), "textures")
SIZE = 32


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


# Small deterministic value-noise so tiles wrap seamlessly.
def vnoise(seed):
    g = {}

    def grad(ix, iy):
        key = (ix % 8, iy % 8)  # wrap at 8 for tileability
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
    """Deterministic per-pixel hash in [0, 1) — scattered, not periodic."""
    n = (x * 374761393 + y * 668265263 + s * 362437) & 0xFFFFFFFF
    n = ((n ^ (n >> 13)) * 1274126177) & 0xFFFFFFFF
    return ((n ^ (n >> 16)) & 0xFFFF) / 65535.0


def tile(name, lo, hi, scale, seed, light=None, dark=None, light_p=0.07, dark_p=0.06):
    n = vnoise(seed)
    px = bytearray()
    for y in range(SIZE):
        for x in range(SIZE):
            v = n(x / SIZE * scale, y / SIZE * scale)
            v2 = n(x / SIZE * scale * 2.3 + 4.0, y / SIZE * scale * 2.3 + 9.0)
            t = max(0.0, min(1.0, v * 0.65 + v2 * 0.35))
            c = lerp(lo, hi, t)
            # Randomly scattered light/dark flecks for a hand-painted look.
            r = rnd(x, y, seed * 7 + 1)
            if light and r < light_p:
                c = light
            elif dark and r > 1.0 - dark_p:
                c = dark
            px += bytes((c[0], c[1], c[2], 255))
    write_png(os.path.join(OUT, name), SIZE, SIZE, px)


def main():
    os.makedirs(OUT, exist_ok=True)
    # WC3-ish warm, saturated, limited palettes with scattered flecks.
    tile("grass.png", (52, 88, 36), (92, 134, 56), 4.0, 1, light=(142, 178, 88), dark=(40, 66, 28))
    tile("dirt.png", (94, 70, 40), (136, 104, 62), 4.0, 2, light=(160, 126, 78), dark=(74, 52, 30))
    tile("rock.png", (76, 72, 68), (122, 116, 106), 5.0, 3, light=(150, 144, 134), dark=(52, 48, 44))
    tile("sand.png", (152, 136, 90), (198, 180, 122), 3.5, 4, light=(214, 198, 142), dark=(132, 116, 74))
    tile("water.png", (26, 68, 108), (44, 112, 152), 3.0, 5, light=(70, 140, 176), dark=(20, 52, 86))
    print("wrote tiles to", OUT)


if __name__ == "__main__":
    main()
