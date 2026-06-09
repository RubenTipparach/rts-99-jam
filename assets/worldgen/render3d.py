#!/usr/bin/env python3
"""Render a voxel world's marching-cubes mesh to a preview screenshot.

A small orthographic, z-buffered Gouraud rasterizer (pure stdlib) so the 3D
terrain can be verified without a GPU: it meshes the density grid with
`marching_cubes`, shades per-vertex from the surface normal, colours by material
from the world's palette, tints the BUILDABLE (flat) ground green, draws the
obvious hazards, and bakes in the world name + measured flat percentage.

Run:  python3 assets/worldgen/render3d.py            # all worlds
      python3 assets/worldgen/render3d.py moon miranda europa
"""

import math
import os
import sys

import common as cm
import worlds as cat
import densitygen as dg
import marching_cubes as mcube
import voxel as vox

IMG_W, IMG_H = 920, 620
MARGIN = 26
TOP = 74  # headroom for the label
EXAG = 4.2  # vertical exaggeration for the preview, so cliffs/plateaus read in 3D
LIGHT = (-0.5, 0.78, 0.38)
PREV_DIR = os.path.join(os.path.dirname(__file__), "..", "..", "docs", "worldgen", "previews")

# Fixed iso-ish camera (a touch lower than the in-game angle so cliffs show).
_YAW, _PITCH = 0.9, 0.82
_DIR = (math.cos(_YAW) * math.cos(_PITCH), math.sin(_PITCH), math.sin(_YAW) * math.cos(_PITCH))


def _norm(v):
    ln = math.sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]) or 1.0
    return (v[0] / ln, v[1] / ln, v[2] / ln)


def _cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


_RIGHT = _norm(_cross((0.0, 1.0, 0.0), _DIR))
_UP = _norm(_cross(_DIR, _RIGHT))
_L = _norm(LIGHT)


LIQUID_COLOR = {"earth": (40, 92, 150), "titan": (62, 46, 30)}


def _material_color(world, pal, mat):
    if mat == dg.MAT_LOW:
        return pal["low"]
    if mat == dg.MAT_HIGH:
        # On icy worlds the cliff/crack network carries the signature accent
        # (Europa's lineae, the grooved-ice sulci); on rock it stays rock.
        if world["archetype"] in ("europa_ice", "grooved_ice", "bright_ice"):
            return pal["accent"]
        return pal["high"]
    if mat == dg.MAT_ACCENT:
        return pal["accent"]
    if mat == dg.MAT_HAZARD:
        if world["key"] == "io":
            return (236, 120, 44)            # lava
        return (206, 228, 242)               # geyser ice
    return pal["mid"]


def render_world(world):
    grid, stats = dg.build(world)
    verts, normals, tris, tmat = mcube.polygonize(grid)
    pal = cat.palette(world)
    tint = world["tint"]
    bright = world["bright"]
    nxz = grid.nx
    x0, _, y0, _, z0, _ = grid.bounds
    dx, dy, dz = grid.dx(), grid.dy(), grid.dz()

    def col_at(vx, vz):
        ci = max(0, min(nxz - 1, int((vx - x0) / dx)))
        ck = max(0, min(nxz - 1, int((vz - z0) / dz)))
        return ci, ck

    def voxel_at(vx, vy, vz):
        ci, ck = col_at(vx, vz)
        cj = max(0, min(grid.ny - 1, int((vy - y0) / dy)))
        return ci, cj, ck

    # Project all verts; auto-fit to the frame, then expose a reusable to_screen.
    proj = []
    minpx = minpy = 1e18
    maxpx = maxpy = -1e18
    for (vx, vy, vz) in verts:
        ey = vy * EXAG
        px = vx * _RIGHT[0] + ey * _RIGHT[1] + vz * _RIGHT[2]
        py = vx * _UP[0] + ey * _UP[1] + vz * _UP[2]
        d = vx * _DIR[0] + ey * _DIR[1] + vz * _DIR[2]
        proj.append((px, py, d))
        minpx = min(minpx, px); maxpx = max(maxpx, px)
        minpy = min(minpy, py); maxpy = max(maxpy, py)
    scale = min((IMG_W - 2 * MARGIN) / (maxpx - minpx or 1),
                (IMG_H - TOP - MARGIN) / (maxpy - minpy or 1))
    ox = (IMG_W - (maxpx - minpx) * scale) * 0.5

    def to_screen(wx, wy, wz):
        ey = wy * EXAG
        px = wx * _RIGHT[0] + ey * _RIGHT[1] + wz * _RIGHT[2]
        py = wx * _UP[0] + ey * _UP[1] + wz * _UP[2]
        d = wx * _DIR[0] + ey * _DIR[1] + wz * _DIR[2]
        return (ox + (px - minpx) * scale, TOP + (maxpy - py) * scale, d)

    screen = [to_screen(*v) for v in verts]

    # Per-vertex lit colour (Gouraud). Buildable ground is pushed green; lava glows.
    vcol = [(0, 0, 0)] * len(verts)
    for vi, (vx, vy, vz) in enumerate(verts):
        ci, cj, ck = voxel_at(vx, vy, vz)
        mat = grid.material[grid.lin(ci, cj, ck)]
        base = _material_color(world, pal, mat)
        n = normals[vi]
        ndl = max(0.0, n[0] * _L[0] + n[1] * _L[1] + n[2] * _L[2])
        sh = 0.32 + 0.85 * ndl
        emissive = mat == dg.MAT_HAZARD and world["key"] == "io"
        if emissive:
            sh = 1.15
        c = (base[0] * tint[0] * bright, base[1] * tint[1] * bright, base[2] * tint[2] * bright)
        if grid.buildable[ck * nxz + ci] == 1:
            c = (c[0] * 0.78 + 70 * 0.22, c[1] * 0.78 + 150 * 0.22, c[2] * 0.78 + 80 * 0.22)
        vcol[vi] = (cm.clamp8(c[0] * sh), cm.clamp8(c[1] * sh), cm.clamp8(c[2] * sh))

    frame = cm.Frame(IMG_W, IMG_H)
    _starfield(frame, world["terrain"]["seed"])
    zbuf = [-1e18] * (IMG_W * IMG_H)
    for (a, b, c) in tris:
        _raster(frame, zbuf, screen[a], screen[b], screen[c], vcol[a], vcol[b], vcol[c])

    _liquid_pass(frame, zbuf, grid, world, to_screen)
    _plumes(frame, grid, stats, to_screen)
    _overlay(frame, world, stats)
    return frame, stats


def _liquid_pass(frame, zbuf, grid, world, to_screen):
    """Draw oceans / lakes / rivers as flat liquid quads, z-buffered under cliffs."""
    base = LIQUID_COLOR.get(world["key"])
    if base is None:
        return
    nxz = grid.nx
    for k in range(nxz - 1):
        for i in range(nxz - 1):
            lj = grid.liquid[k * nxz + i]
            if lj == 0:
                continue
            ly = grid.world(i, lj - 1, k)[1]
            p00 = to_screen(*_xz(grid, i, k, ly))
            p10 = to_screen(*_xz(grid, i + 1, k, ly))
            p11 = to_screen(*_xz(grid, i + 1, k + 1, ly))
            p01 = to_screen(*_xz(grid, i, k + 1, ly))
            # subtle ripple sparkle so large bodies are not dead flat
            spark = cm.value_noise(i * 0.7, k * 0.7, 5) * 0.25
            col = cm.lerp3(base, (205, 222, 240), spark)
            _raster(frame, zbuf, p00, p10, p11, col, col, col)
            _raster(frame, zbuf, p00, p11, p01, col, col, col)


def _xz(grid, i, k, y):
    x0, _, _, _, z0, _ = grid.bounds
    return (x0 + i * grid.dx(), y, z0 + k * grid.dz())


def _plumes(frame, grid, stats, to_screen):
    """Volcanic and geyser plumes rising from their vents."""
    nxz = grid.nx
    for (i, k, kind) in stats.get("vents", []):
        # surface height at the vent column
        sy_top = grid.bounds[2]
        for j in range(grid.ny - 1, -1, -1):
            if grid.density[grid.lin(i, j, k)] >= vox.ISO:
                sy_top = grid.world(i, j, k)[1]
                break
        sx, syy, _ = to_screen(*_xz(grid, i, k, sy_top))
        sx, syy = int(sx), int(syy)
        if kind == "volcano":
            col, height, spread0, alpha0 = (255, 150, 60), 90, 2.0, 0.85
        else:  # geyser (mini cryo-cone)
            col, height, spread0, alpha0 = (210, 232, 246), 44, 1.1, 0.5
        for t in range(height):
            f = t / height
            spread = spread0 + f * (4.0 if kind == "geyser" else 3.0)
            a = (1.0 - f) ** 1.5 * alpha0
            yy = syy - t
            for dxp in range(-int(spread), int(spread) + 1):
                fall = 1.0 - abs(dxp) / (spread + 0.5)
                frame.blend(sx + dxp, yy, col, a * fall)


def _raster(frame, zbuf, s0, s1, s2, c0, c1, c2):
    x0, y0, d0 = s0
    x1, y1, d1 = s1
    x2, y2, d2 = s2
    minx = max(0, int(min(x0, x1, x2)))
    maxx = min(IMG_W - 1, int(max(x0, x1, x2)) + 1)
    miny = max(0, int(min(y0, y1, y2)))
    maxy = min(IMG_H - 1, int(max(y0, y1, y2)) + 1)
    if minx > maxx or miny > maxy:
        return
    area = (x1 - x0) * (y2 - y0) - (x2 - x0) * (y1 - y0)
    if abs(area) < 1e-9:
        return
    inv = 1.0 / area
    buf = frame.buf
    W = IMG_W
    for y in range(miny, maxy + 1):
        py = y + 0.5
        row = y * W
        for x in range(minx, maxx + 1):
            px = x + 0.5
            w0 = ((x1 - px) * (y2 - py) - (x2 - px) * (y1 - py)) * inv
            w1 = ((x2 - px) * (y0 - py) - (x0 - px) * (y2 - py)) * inv
            w2 = 1.0 - w0 - w1
            if w0 < 0 or w1 < 0 or w2 < 0:
                continue
            depth = w0 * d0 + w1 * d1 + w2 * d2
            zi = row + x
            if depth <= zbuf[zi]:
                continue
            zbuf[zi] = depth
            i = zi * 3
            buf[i] = int(w0 * c0[0] + w1 * c1[0] + w2 * c2[0])
            buf[i + 1] = int(w0 * c0[1] + w1 * c1[1] + w2 * c2[1])
            buf[i + 2] = int(w0 * c0[2] + w1 * c1[2] + w2 * c2[2])


def _starfield(frame, seed):
    for y in range(frame.h):
        col = cm.lerp3((7, 8, 14), (11, 9, 17), y / frame.h)
        r, g, b = col
        row = y * frame.w
        for x in range(frame.w):
            i = (row + x) * 3
            frame.buf[i] = r; frame.buf[i + 1] = g; frame.buf[i + 2] = b
    rng = cm.Rng(seed * 7 + 1)
    for _ in range(int(frame.w * frame.h * 0.0014)):
        frame.put(rng.randint(0, frame.w - 1), rng.randint(0, frame.h - 1),
                  cm.scale3((200, 205, 225), rng.uniform(0.3, 1.0)))


def _overlay(frame, world, stats):
    cm.draw_text(frame, 18, 16, world["name"], (240, 240, 246), scale=3)
    cm.draw_text(frame, 20, 48, "marching-cubes voxel map", (150, 170, 200), scale=1)
    flat = stats["flat"] * 100
    cm.draw_text(frame, IMG_W - 250, 16, f"buildable {flat:4.1f}%", (140, 220, 150), scale=2)
    cm.draw_text(frame, IMG_W - 250, 40, f"caves {stats['caves']}  step {stats['step']:.0f}",
                 (150, 170, 200), scale=1)
    cm.draw_text(frame, 18, frame.h - 20, "green = flat / buildable ground", (140, 200, 150), scale=1)


def main():
    os.makedirs(PREV_DIR, exist_ok=True)
    keys = [a for a in sys.argv[1:] if not a.startswith("-")]
    targets = [cat.by_key(k) for k in keys] if keys else cat.WORLDS
    frames = []
    for w in targets:
        fr, st = render_world(w)
        out = os.path.join(PREV_DIR, w["key"] + ".png")
        fr.save(out)
        frames.append(fr)
        print(f"  {w['key']:10s} buildable {st['flat']*100:4.1f}%  -> {os.path.relpath(out)}")
    if not keys:
        cm.contact_sheet(frames, os.path.join(os.path.dirname(PREV_DIR), "contact_sheet.png"))
        print("  contact sheet -> docs/worldgen/contact_sheet.png")


if __name__ == "__main__":
    main()
