#!/usr/bin/env python3
"""Generate tileable texture sets, one per archetype, under
`assets/textures/worlds/<archetype>/`.

Each set has four 64x64 tiles that drop straight into the client's existing
four-sampler terrain shader (`apps/client/src/shader.wgsl`), which blends a base
surface with a low/lowland tile, a high/slope tile and an accent tile:

  base.png   - the dominant surface (regolith, ice plain, sulfur, dust)
  low.png    - basins / mare / shadowed lowland / dark hemisphere
  high.png   - sunlit highs, ridges, fresh ice, slope rock
  accent.png - the defining splash: ejecta rays, lineae, lava, salts, dunes, lakes

Tiles are linear-sampled in engine (a soft painterly blur), painted with
wrapping value noise plus scattered flecks, exactly like `assets/gen_textures.py`.
"""

import math
import os
import zlib

from common import write_png, clamp8, lerp3, scale3
import worlds as cat

SIZE = 64
OUT_ROOT = os.path.join(os.path.dirname(__file__), "..", "textures", "worlds")


def stable_seed(s):
    """A process-independent integer seed from a string (builtin hash() is
    salted per run, so it can't be used for reproducible assets)."""
    return zlib.crc32(s.encode("utf-8"))


def _wrap_noise(period, seed):
    """Value noise whose integer lattice wraps every `period` cells, so a tile
    sampled over [0, period) is seamless."""
    cache = {}

    def grad(ix, iy):
        key = (ix % period, iy % period)
        if key not in cache:
            n = (key[0] * 1619 + key[1] * 31337 + seed * 1013) & 0x7FFFFFFF
            n = (n ^ (n >> 13)) * 1274126177 & 0x7FFFFFFF
            cache[key] = (n % 1000) / 1000.0
        return cache[key]

    def at(x, y):
        ix, iy = math.floor(x), math.floor(y)
        fx, fy = x - ix, y - iy
        a, b = grad(ix, iy), grad(ix + 1, iy)
        c, d = grad(ix, iy + 1), grad(ix + 1, iy + 1)
        ux = fx * fx * (3 - 2 * fx)
        uy = fy * fy * (3 - 2 * fy)
        top = a + (b - a) * ux
        bot = c + (d - c) * ux
        return top + (bot - top) * uy

    return at


def _fleck(x, y, seed):
    n = (x * 374761393 + y * 668265263 + seed * 362437) & 0xFFFFFFFF
    n = ((n ^ (n >> 13)) * 1274126177) & 0xFFFFFFFF
    return ((n ^ (n >> 16)) & 0xFFFF) / 65535.0


def make_tile(path, lo, hi, scale, seed, light=None, dark=None,
              light_p=0.06, dark_p=0.06, streak=0.0, streak_dir=0.0):
    """Paint one tileable tile blending `lo`->`hi` by noise, with optional flecks
    and an optional directional streak overlay (for grooves / dunes / lineae)."""
    period = max(2, int(round(scale)))
    n = _wrap_noise(period, seed)
    n2 = _wrap_noise(period * 2, seed + 7)
    px = bytearray()
    sd, cd = math.sin(streak_dir), math.cos(streak_dir)
    for y in range(SIZE):
        for x in range(SIZE):
            u = x / SIZE * scale
            v = y / SIZE * scale
            t = n(u, v) * 0.62 + n2(u * 2.0 + 4.0, v * 2.0 + 9.0) * 0.38
            t = max(0.0, min(1.0, t))
            c = lerp3(lo, hi, t)
            if streak > 0.0:
                # A ridged sinusoid along streak_dir, jittered by noise.
                s = (x * cd + y * sd) * (scale / SIZE) * 3.14159 + n(u, v) * 2.0
                ridge = abs(math.sin(s))
                c = lerp3(c, hi, streak * (1.0 - ridge) * 0.8)
            r = _fleck(x, y, seed * 7 + 1)
            if light and r < light_p:
                c = light
            elif dark and r > 1.0 - dark_p:
                c = dark
            px += bytes((c[0], c[1], c[2]))
    write_png(path, SIZE, SIZE, px, alpha=False)


def generate():
    made = []
    for arch, pal in cat.ARCHETYPES.items():
        out = os.path.join(OUT_ROOT, arch)
        os.makedirs(out, exist_ok=True)
        scale = pal["scale"]
        low, mid, high = pal["low"], pal["mid"], pal["high"]
        accent, dark = pal["accent"], pal["dark"]

        # base: around the mid tone, with dark/high flecks for grain.
        make_tile(os.path.join(out, "base.png"),
                  scale3(mid, 0.86), scale3(mid, 1.12), scale, 100 + stable_seed(arch) % 97,
                  light=scale3(high, 1.0), dark=dark, light_p=0.05, dark_p=0.06)
        # low: basins / dark hemisphere, blending toward mid.
        make_tile(os.path.join(out, "low.png"),
                  scale3(low, 0.85), lerp3(low, mid, 0.5), scale * 0.9, 200 + stable_seed(arch) % 89,
                  light=mid, dark=scale3(low, 0.7), light_p=0.03, dark_p=0.08)
        # high: ridges / fresh ice / sunlit slopes.
        make_tile(os.path.join(out, "high.png"),
                  lerp3(mid, high, 0.4), high, scale * 1.1, 300 + stable_seed(arch) % 83,
                  light=scale3(high, 1.08), dark=mid, light_p=0.07, dark_p=0.04)
        # accent: the archetype's signature material. Streaked for the icy /
        # grooved / dune / lineae kits so it reads as linear features.
        streaked = arch in ("grooved_ice", "europa_ice", "titan_haze",
                            "bright_ice", "triton_ice")
        make_tile(os.path.join(out, "accent.png"),
                  scale3(accent, 0.72), scale3(accent, 1.22), scale * 0.8,
                  400 + stable_seed(arch) % 79,
                  light=scale3(accent, 1.35), dark=scale3(accent, 0.5),
                  light_p=0.06, dark_p=0.06,
                  streak=0.5 if streaked else 0.0, streak_dir=0.7)
        made.append(arch)
    return made


if __name__ == "__main__":
    names = generate()
    print(f"wrote {len(names)} texture sets (4 tiles each) to {os.path.normpath(OUT_ROOT)}")
    for a in names:
        print(f"  {a}")
