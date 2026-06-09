#!/usr/bin/env python3
"""Procedural terrain generation + an offline previewer.

For each world this:
  1. builds a height field and a stack of material / hazard masks from the
     world's recipe in `worlds.py` (the same recipe the engine procgen mirrors),
  2. shades it with the world's texture set (read straight back from
     `assets/textures/worlds/<archetype>/`), and
  3. renders it as an oblique, hill-shaded 3D chunk floating in space, with
     hazards drawn on top (lava glow, methane lakes, cryo-geysers, ice rifts,
     dust storms, ...) and a label / hazard legend baked in.

The result is a per-world PNG under `docs/worldgen/previews/` plus a contact
sheet, so the procgen and texture work can be eyeballed without a GPU. Pure
stdlib; no external renderer.

Run:  python3 assets/worldgen/render.py          (all worlds)
      WORLDGEN_QUICK=1 python3 .../render.py      (fast, low-res smoke test)
      python3 .../render.py moon io europa         (a subset, by key)
"""

import math
import os
import sys

import common as cm
import worlds as cat
import textures as tx

QUICK = os.environ.get("WORLDGEN_QUICK") == "1"
N = 96 if QUICK else 168           # terrain grid resolution (cells per side)
IMG_W, IMG_H = (560, 380) if QUICK else (960, 640)
PREV_DIR = os.path.join(os.path.dirname(__file__), "..", "..", "docs", "worldgen", "previews")

# Oblique projection constants (a rotated, tilted heightfield).
PROJ_A = IMG_W * 0.455 / N          # half-diagonal x spread per cell
PROJ_B = IMG_H * 0.300 / N          # diagonal y spread per cell
CX = IMG_W * 0.5
CY = IMG_H * 0.235
VERT = (60.0 if QUICK else 96.0)    # vertical exaggeration (pixels per relief unit)
LIGHT = (-0.55, -0.5, 0.65)         # light from upper-left, slightly toward camera


# --------------------------------------------------------------------------- #
# texture-set cache
# --------------------------------------------------------------------------- #
_TILES = {}


def tiles_for(arch):
    if arch not in _TILES:
        root = os.path.join(tx.OUT_ROOT, arch)
        slots = {}
        for slot in ("base", "low", "high", "accent"):
            slots[slot] = cm.read_png(os.path.join(root, slot + ".png"))
        _TILES[arch] = slots
    return _TILES[arch]


def sample(tile, fx, fy):
    w, h, ch, buf = tile
    x = int(fx) % w
    y = int(fy) % h
    i = (y * w + x) * ch
    return (buf[i], buf[i + 1], buf[i + 2])


# --------------------------------------------------------------------------- #
# procedural terrain + masks
# --------------------------------------------------------------------------- #
def _basin(u, v, seed):
    """Large smooth lowland blobs in [0, 1] (seas, planitia, mare regions)."""
    m = cm.fbm(u * 1.6 + 5.0, v * 1.6 + 9.0, seed + 40, octaves=3)
    return cm.clamp((m - 0.45) / 0.35, 0.0, 1.0)


def _cantaloupe(u, v, seed):
    """Triton-style dimpled terrain: packed rounded pits."""
    k = 7.0
    best = 1e9
    cu, cv = u * k, v * k
    bi, bj = math.floor(cu), math.floor(cv)
    for dj in (-1, 0, 1):
        for di in (-1, 0, 1):
            gx = bi + di
            gy = bj + dj
            jx = gx + 0.5 + (cm._hash2(gx, gy, seed) - 0.5) * 0.7
            jy = gy + 0.5 + (cm._hash2(gx, gy, seed + 3) - 0.5) * 0.7
            d = (cu - jx) ** 2 + (cv - jy) ** 2
            if d < best:
                best = d
    return -math.exp(-best * 2.2)  # negative: pits


def build(world):
    """Return a dict of flat arrays describing the world's surface."""
    t = world["terrain"]
    seed = t["seed"]
    relief = t["relief"]
    fscale = 3.4 + t["roughness"] * 1.2
    sd, cd = math.sin(t["groove_dir"]), math.cos(t["groove_dir"])
    dsd, dcd = math.sin(t["dune_dir"]), math.cos(t["dune_dir"])
    plains_level = -0.15

    VN = N + 1
    vh = [0.0] * (VN * VN)
    for j in range(VN):
        v = j / N
        for i in range(VN):
            u = i / N
            wu = u + (cm.fbm(u * 2.0, v * 2.0, seed + 5, 3) - 0.5) * t["warp"] * 0.35
            wv = v + (cm.fbm(u * 2.0 + 3, v * 2.0 + 1, seed + 6, 3) - 0.5) * t["warp"] * 0.35
            h = (cm.fbm(wu * fscale, wv * fscale, seed, 5) - 0.5) * 2.0
            h *= (1.0 - t["smoothness"] * 0.65)
            if t["grooves"] > 0:
                r = cm.ridged(u * cd * 6 - v * sd * 6, u * sd * 6 + v * cd * 6, seed + 12, 4)
                h += (r - 0.45) * t["grooves"] * 0.7
            if t["dunes"] > 0:
                phase = (u * dcd + v * dsd) * 46.0 + cm.fbm(u * 3, v * 3, seed + 8, 2) * 5.0
                h += math.sin(phase) * t["dunes"] * 0.16
            if t["cantaloupe"] > 0:
                h += _cantaloupe(u, v, seed + 14) * t["cantaloupe"] * 0.6
            if t["ridge"] > 0:
                band = math.exp(-((v - 0.5) / 0.045) ** 2)
                h += band * t["ridge"] * 0.9
            if t["plains"] > 0:
                m = _basin(u, v, seed)
                h = cm.lerp(h, plains_level, m * t["plains"])
            vh[j * VN + i] = h * relief

    # cell-resolution masks
    cf = [0.0] * (N * N)   # crater floor
    cr = [0.0] * (N * N)   # crater rim / ejecta rays
    rift = [0.0] * (N * N)
    groove = [0.0] * (N * N)  # bright sulci bands (colour, matches the height)
    lava = [0.0] * (N * N)
    lake = [0.0] * (N * N)
    geyser = [0.0] * (N * N)
    brine = [0.0] * (N * N)
    plains = [0.0] * (N * N)
    frost = [0.0] * (N * N)
    dark = [0.0] * (N * N)
    vents = []  # (i, j, kind) for plume/geyser/lava post overlays

    # ---- craters: rasterized into the height field + masks ----
    rng = cm.Rng(seed * 131 + 17)
    area = N * N
    count = int(area * 0.0009 * t["crater_density"] * (1.0 - t["smoothness"] * 0.7))
    for _ in range(count):
        cxp = rng.uniform(0, N)
        cyp = rng.uniform(0, N)
        rad = rng.uniform(t["crater_min"], t["crater_max"]) * N
        if rad < 1.2:
            continue
        depth = rad * 0.06 * relief
        rimh = depth * 0.7
        r0 = int(max(0, cxp - rad * 1.6))
        r1 = int(min(N, cxp + rad * 1.6))
        c0 = int(max(0, cyp - rad * 1.6))
        c1 = int(min(N, cyp + rad * 1.6))
        for j in range(c0, c1):
            for i in range(r0, r1):
                d = math.hypot(i + 0.5 - cxp, j + 0.5 - cyp) / rad
                idx = j * N + i
                if d <= 1.0:
                    cavity = depth * (d * d - 1.0)
                    rim = rimh * math.exp(-((d - 0.92) / 0.16) ** 2)
                    vh[j * VN + i] += cavity + rim
                    if d < 0.72:
                        cf[idx] = max(cf[idx], 1.0 - d)
                    if d > 0.8:
                        cr[idx] = max(cr[idx], 1.0)
                elif d <= 1.5:
                    cr[idx] = max(cr[idx], (1.5 - d) / 0.5)

    # ---- continuous masks (gated by the world's hazards/recipe) ----
    haz = {h["kind"]: h for h in world["hazards"]}

    def has(k):
        return k in haz

    for j in range(N):
        v = (j + 0.5) / N
        for i in range(N):
            u = (i + 0.5) / N
            idx = j * N + i

            if t["rifts"] > 0:
                # A few long fractures: thin where a warped sinus crosses zero.
                rr = math.sin((u * 1.7 + v * 2.3) * 6.0 + cm.fbm(u * 2, v * 2, seed + 20, 3) * 6.0)
                rr += math.sin((u * 2.9 - v * 1.3) * 5.0 + cm.fbm(u * 2 + 7, v * 2, seed + 21, 3) * 6.0)
                line = max(0.0, 1.0 - abs(rr) * 3.0)
                rift[idx] = line * t["rifts"]

            if t["grooves"] > 0:
                gr = cm.ridged(u * cd * 6 - v * sd * 6, u * sd * 6 + v * cd * 6, seed + 12, 4)
                groove[idx] = cm.clamp((gr - 0.5) * 2.4, 0, 1) * t["grooves"]

            if t["calderas"] > 0:
                cmask = cm.fbm(u * 5.0, v * 5.0, seed + 30, 3)
                if cmask > 0.62:
                    lv = (cmask - 0.62) / 0.38
                    lava[idx] = lv
                    vh[j * VN + i] -= lv * 0.12 * relief

            if has("hydro_lake"):
                b = _basin(u, v, seed + 2)
                if b > 0.42:
                    lk = cm.clamp((b - 0.42) / 0.5, 0, 1) * haz["hydro_lake"]["intensity"]
                    lake[idx] = lk
                    # flatten the basin into a still, level sea
                    sea = -0.2 * relief
                    for (jj, ii) in ((j, i), (j, i + 1), (j + 1, i), (j + 1, i + 1)):
                        vi = jj * VN + ii
                        vh[vi] = cm.lerp(vh[vi], min(vh[vi], sea), lk * 0.9)

            if has("plains") or t["plains"] > 0:
                plains[idx] = _basin(u, v, seed)

            if has("brine"):
                # bright salt patches keyed to a sparse high-frequency mask
                s = cm.value_noise(u * 22, v * 22, seed + 33)
                if s > 0.93:
                    brine[idx] = 1.0

            if has("frost"):
                # polar caps: brighten near top/bottom rows
                cap = max(0.0, 1.0 - v / 0.16) + max(0.0, (v - 0.84) / 0.16)
                frost[idx] = cm.clamp(cap, 0, 1)

            if has("dichotomy"):
                # one hemisphere darkened (Iapetus / Cassini Regio)
                dd = cm.clamp((0.5 - u) * 3.0 + (cm.fbm(u * 3, v * 3, seed + 9, 2) - 0.5), 0, 1)
                dark[idx] = dd * haz["dichotomy"]["intensity"]

            if has("cryogeyser"):
                # vents ride the rift lines, concentrated toward the south pole
                pole = math.exp(-((v - 0.86) / 0.16) ** 2) if world["key"] == "enceladus" else 0.5
                geyser[idx] = rift[idx] * pole

    # ---- pick a handful of vent positions for plume overlays ----
    def pick_vents(field, kind, n):
        cand = sorted(range(N * N), key=lambda k: field[k], reverse=True)
        step = max(1, N // 6)
        chosen = []
        for k in cand:
            if field[k] < 0.35:
                break
            ii, jj = k % N, k // N
            if all(abs(ii - a) + abs(jj - b) > step for a, b, _ in chosen):
                chosen.append((ii, jj, kind))
            if len(chosen) >= n:
                break
        return chosen

    if has("lava") or t["calderas"] > 0:
        vents += pick_vents(lava, "lava", 4)
    if has("cryogeyser"):
        vents += pick_vents(geyser, "geyser", 5)
    if has("outgassing"):
        # comet jets: a couple near the limb
        vents += [(int(N * 0.3), int(N * 0.25), "jet"), (int(N * 0.62), int(N * 0.4), "jet")]
    if has("plume"):
        vents += pick_vents(lava, "plume", 2)

    # elevation normalization for palette ramps
    lo = min(vh)
    hi = max(vh)
    span = (hi - lo) or 1.0

    return dict(vh=vh, VN=VN, cf=cf, cr=cr, rift=rift, groove=groove, lava=lava,
                lake=lake, geyser=geyser, brine=brine, plains=plains, frost=frost,
                dark=dark, vents=vents, lo=lo, span=span, haz=haz)


# --------------------------------------------------------------------------- #
# shading + projection
# --------------------------------------------------------------------------- #
def project(i, j, h):
    sx = CX + (i - j) * PROJ_A
    sy = CY + (i + j) * PROJ_B - h * VERT
    return (sx, sy)


def fill_quad(frame, pts, color):
    ys = [p[1] for p in pts]
    y0 = max(0, int(math.floor(min(ys))))
    y1 = min(frame.h - 1, int(math.ceil(max(ys))))
    n = len(pts)
    for y in range(y0, y1 + 1):
        yc = y + 0.5
        xs = []
        for k in range(n):
            ax, ay = pts[k]
            bx, by = pts[(k + 1) % n]
            if (ay <= yc < by) or (by <= yc < ay):
                xs.append(ax + (bx - ax) * (yc - ay) / (by - ay))
        if len(xs) >= 2:
            xa = max(0, int(math.floor(min(xs))))
            xb = min(frame.w - 1, int(math.ceil(max(xs))))
            row = y * frame.w
            for x in range(xa, xb + 1):
                idx = (row + x) * 3
                frame.buf[idx] = color[0]
                frame.buf[idx + 1] = color[1]
                frame.buf[idx + 2] = color[2]


def cell_color(world, f, tiles, pal, i, j):
    VN = f["VN"]
    vh = f["vh"]
    idx = j * N + i
    h00 = vh[j * VN + i]
    h10 = vh[j * VN + i + 1]
    h01 = vh[(j + 1) * VN + i]
    h11 = vh[(j + 1) * VN + i + 1]
    havg = (h00 + h10 + h01 + h11) * 0.25
    en = cm.clamp((havg - f["lo"]) / f["span"], 0.0, 1.0)

    # hillshade from the cell's slope
    dzdx = (h10 + h11) - (h00 + h01)
    dzdy = (h01 + h11) - (h00 + h10)
    nx, ny, nz = -dzdx * 2.2, -dzdy * 2.2, 1.0
    nl = math.sqrt(nx * nx + ny * ny + nz * nz) or 1.0
    ndl = (nx * LIGHT[0] + ny * LIGHT[1] + nz * LIGHT[2]) / (nl * 1.0)
    shade = cm.clamp(0.34 + 0.82 * cm.clamp(ndl, 0, 1), 0.2, 1.25)
    slope = cm.clamp(math.hypot(dzdx, dzdy) * 3.5, 0, 1)

    # base texel: choose tile by elevation, push to "high" on steep slopes
    ts = N / 9.0
    fx, fy = i * ts / N * tiles["base"][0], j * ts / N * tiles["base"][1]
    if en < 0.34:
        c = sample(tiles["low"], i * 1.7, j * 1.7)
    elif en < 0.68:
        c = sample(tiles["base"], i * 1.7, j * 1.7)
    else:
        c = sample(tiles["high"], i * 1.7, j * 1.7)
    if slope > 0.4:
        c = cm.lerp3(c, sample(tiles["high"], i * 1.9 + 5, j * 1.9 + 5), (slope - 0.4) * 1.2)

    # craters: dark smooth floors, bright ejecta/ray rims (accent tile)
    if f["cf"][idx] > 0:
        c = cm.scale3(c, 1.0 - 0.35 * f["cf"][idx])
    if f["cr"][idx] > 0 and world["archetype"] not in ("io_sulfur",):
        c = cm.lerp3(c, sample(tiles["accent"], i * 2.3, j * 2.3), 0.35 * f["cr"][idx])

    # bright icy sulci bands (Ganymede / Ariel / Miranda)
    if f["groove"][idx] > 0:
        band = sample(tiles["high"], i * 2.0 + 3, j * 2.0 + 3)
        c = cm.lerp3(c, band, cm.clamp(f["groove"][idx], 0, 1) * 0.55)

    # grooved / lineae accent along rifts (icy worlds)
    if f["rift"][idx] > 0:
        acc = sample(tiles["accent"], i * 2.1, j * 2.1)
        c = cm.lerp3(c, acc, cm.clamp(f["rift"][idx], 0, 1) * 0.7)

    glow = None  # emissive add (lava/geyser), applied after shading

    if f["lava"][idx] > 0:
        lv = f["lava"][idx]
        molten = cm.lerp3((70, 30, 18), (255, 170, 60), cm.clamp(lv * 1.3, 0, 1))
        c = cm.lerp3(c, molten, cm.clamp(lv * 1.4, 0, 1))
        glow = cm.scale3((255, 120, 40), lv)

    if f["lake"][idx] > 0:
        lk = f["lake"][idx]
        c = cm.lerp3(c, (22, 26, 40), cm.clamp(lk * 2.0, 0, 1))
        shade = cm.lerp(shade, 0.7, cm.clamp(lk * 1.5, 0, 1))  # flat, dark, mirror-like

    if f["geyser"][idx] > 0:
        c = cm.lerp3(c, (150, 195, 220), cm.clamp(f["geyser"][idx], 0, 1) * 0.6)

    if f["brine"][idx] > 0:
        c = cm.lerp3(c, (220, 225, 230), 0.85)

    if f["dark"][idx] > 0:
        c = cm.scale3(c, 1.0 - 0.6 * f["dark"][idx])

    if f["plains"][idx] > 0 and (world["key"] in ("pluto", "triton")):
        pl = f["plains"][idx]
        nitro = (228, 214, 190) if world["key"] == "pluto" else (224, 206, 198)
        c = cm.lerp3(c, nitro, cm.clamp(pl * 0.8, 0, 1))
        shade = cm.lerp(shade, 0.95, pl * 0.6)

    if f["frost"][idx] > 0:
        c = cm.lerp3(c, (230, 234, 240), f["frost"][idx] * 0.8)

    # world tint + brightness, then shade
    tint = world["tint"]
    b = world["bright"]
    c = (cm.clamp8(c[0] * tint[0] * b),
         cm.clamp8(c[1] * tint[1] * b),
         cm.clamp8(c[2] * tint[2] * b))
    c = cm.scale3(c, shade)
    if glow:
        c = cm.add3(c, cm.scale3(glow, 0.6))
    return c


# --------------------------------------------------------------------------- #
# background + overlays
# --------------------------------------------------------------------------- #
def starfield(frame, seed):
    top = (6, 7, 13)
    bot = (10, 8, 16)
    for y in range(frame.h):
        t = y / frame.h
        col = cm.lerp3(top, bot, t)
        row = y * frame.w
        r, g, bl = col
        for x in range(frame.w):
            i = (row + x) * 3
            frame.buf[i] = r
            frame.buf[i + 1] = g
            frame.buf[i + 2] = bl
    rng = cm.Rng(seed * 7 + 1)
    for _ in range(int(frame.w * frame.h * 0.0016)):
        x = rng.randint(0, frame.w - 1)
        y = rng.randint(0, frame.h - 1)
        b = rng.uniform(0.3, 1.0)
        col = cm.scale3((200, 205, 225), b)
        frame.put(x, y, col)
        if b > 0.85:
            frame.blend(x + 1, y, col, 0.4)
            frame.blend(x, y + 1, col, 0.4)


def atmosphere(frame, color, intensity):
    """Soft halo behind the body for worlds with an atmosphere/haze."""
    cx = CX
    cy = CY + N * PROJ_B
    rad = N * PROJ_A * 1.18
    for y in range(int(cy - rad), int(cy + rad)):
        if y < 0 or y >= frame.h:
            continue
        for x in range(int(cx - rad), int(cx + rad)):
            if x < 0 or x >= frame.w:
                continue
            d = math.hypot(x - cx, (y - cy) * (PROJ_A / PROJ_B)) / rad
            if d > 1.0:
                continue
            a = (1.0 - d) ** 2 * intensity * 0.5
            frame.blend(x, y, color, a)


def draw_plumes(frame, world, f):
    for (i, j, kind) in f["vents"]:
        VN = f["VN"]
        h = f["vh"][j * VN + i]
        sx, sy = project(i + 0.5, j + 0.5, h)
        sx, sy = int(sx), int(sy)
        if kind == "lava":
            col = (255, 140, 55)
            height = 26
        elif kind == "geyser":
            col = (205, 225, 240)
            height = 70
        elif kind == "plume":
            col = (235, 215, 235)
            height = 80
        else:  # comet jet
            col = (210, 220, 235)
            height = 60
        for k in range(height):
            t = k / height
            spread = 1 + t * (5 if kind != "lava" else 2)
            a = (1.0 - t) ** 1.6 * (0.9 if kind == "lava" else 0.55)
            yy = sy - k
            for dx in range(-int(spread), int(spread) + 1):
                fall = 1.0 - abs(dx) / (spread + 0.5)
                frame.blend(sx + dx, yy, col, a * fall)


def dust_overlay(frame, color, intensity, seed):
    """A translucent wind-blown haze sweeping across the disc (Mars/Titan)."""
    cx = CX
    cy = CY + N * PROJ_B
    rad = N * PROJ_A
    for y in range(int(cy - rad), int(cy + rad)):
        if y < 0 or y >= frame.h:
            continue
        for x in range(int(cx - rad * 1.1), int(cx + rad * 1.1)):
            if x < 0 or x >= frame.w:
                continue
            d = math.hypot(x - cx, (y - cy) * (PROJ_A / PROJ_B)) / rad
            if d > 1.0:
                continue
            n = cm.fbm(x * 0.012 + 10, y * 0.02, seed + 50, 3)
            a = cm.clamp((n - 0.5) * 2.0, 0, 1) * intensity * 0.4 * (1.0 - d)
            frame.blend(x, y, color, a)


HAZARD_LABEL = {
    "lava": "lava lakes", "hydro_lake": "methane seas", "cryogeyser": "cryo-geysers",
    "ice_rift": "ice rifts", "chaos": "chaos terrain", "radiation": "radiation",
    "dust_storm": "dust storms", "brine": "brine eruptions", "scarp": "cliffs/scarps",
    "outgassing": "comet jets", "frost": "polar frost", "ridge": "equatorial ridge",
    "dichotomy": "albedo dichotomy", "plains": "nitrogen glaciers", "mare": "basalt maria",
    "rays": "ray craters", "plume": "volcanic plumes", "dark_floor": "dark crater floors",
}


def overlay_text(frame, world):
    cm.draw_text(frame, 18, 16, world["name"], (240, 240, 246), scale=3)
    cm.draw_text(frame, 20, 48, world["archetype"].replace("_", " "), (150, 170, 200), scale=1)
    cm.draw_text(frame, 20, 62, "nasa ref: " + world["nasa"][:40], (120, 135, 160), scale=1)
    # hazard legend, lower-left
    y = frame.h - 16 - 12 * len(world["hazards"])
    cm.draw_text(frame, 18, y - 16, "map hazards", (235, 200, 140), scale=1)
    for h in world["hazards"]:
        label = HAZARD_LABEL.get(h["kind"], h["kind"])
        note = h.get("note")
        line = "- " + label + (": " + note if note else "")
        cm.draw_text(frame, 18, y, line[:52], (200, 205, 215), scale=1)
        y += 12


# --------------------------------------------------------------------------- #
# per-world render
# --------------------------------------------------------------------------- #
def render_world(world):
    f = build(world)
    pal = cat.palette(world)
    tiles = tiles_for(world["archetype"])
    frame = cm.Frame(IMG_W, IMG_H)
    starfield(frame, world["terrain"]["seed"])

    # atmosphere haloes before the body
    if world["key"] == "titan":
        atmosphere(frame, (210, 140, 60), 1.0)
    elif world["key"] == "mars":
        atmosphere(frame, (200, 150, 120), 0.5)
    elif world["key"] == "pluto":
        atmosphere(frame, (150, 140, 170), 0.6)
    elif world["key"] == "io":
        atmosphere(frame, (210, 190, 120), 0.4)

    # terrain, back-to-front (increasing i+j)
    for s in range(0, 2 * (N - 1) + 1):
        i0 = max(0, s - (N - 1))
        i1 = min(N - 1, s)
        for i in range(i0, i1 + 1):
            j = s - i
            VN = f["VN"]
            vh = f["vh"]
            p00 = project(i, j, vh[j * VN + i])
            p10 = project(i + 1, j, vh[j * VN + i + 1])
            p11 = project(i + 1, j + 1, vh[(j + 1) * VN + i + 1])
            p01 = project(i, j + 1, vh[(j + 1) * VN + i])
            col = cell_color(world, f, tiles, pal, i, j)
            fill_quad(frame, [p00, p10, p11, p01], col)

    # hazard plumes / jets, then sweeping haze, then labels
    draw_plumes(frame, world, f)
    if "dust_storm" in f["haz"]:
        color = (200, 150, 110) if world["key"] == "mars" else (190, 140, 80)
        dust_overlay(frame, color, f["haz"]["dust_storm"]["intensity"], world["terrain"]["seed"])
    overlay_text(frame, world)
    return frame


# --------------------------------------------------------------------------- #
# contact sheet
# --------------------------------------------------------------------------- #
def contact_sheet(frames, path, cols=3):
    if not frames:
        return
    shrink = 1 if QUICK else 2
    thumbs = [fr.downscaled(shrink) if shrink > 1 else fr for fr in frames]
    tw, th = thumbs[0].w, thumbs[0].h
    rows = (len(thumbs) + cols - 1) // cols
    pad = 6
    sheet = cm.Frame(cols * tw + (cols + 1) * pad, rows * th + (rows + 1) * pad, bg=(8, 9, 14))
    for k, fr in enumerate(thumbs):
        r, c = divmod(k, cols)
        sheet.blit(fr, pad + c * (tw + pad), pad + r * (th + pad))
    sheet.save(path)


def main():
    os.makedirs(PREV_DIR, exist_ok=True)
    keys = [a for a in sys.argv[1:] if not a.startswith("-")]
    targets = [cat.by_key(k) for k in keys] if keys else cat.WORLDS
    frames = []
    for w in targets:
        fr = render_world(w)
        out = os.path.join(PREV_DIR, w["key"] + ".png")
        fr.save(out)
        frames.append(fr)
        print(f"  rendered {w['key']:10s} -> {os.path.relpath(out)}")
    if not keys:
        sheet = os.path.join(os.path.dirname(PREV_DIR), "contact_sheet.png")
        contact_sheet(frames, sheet)
        print(f"  contact sheet -> {os.path.relpath(sheet)}")


if __name__ == "__main__":
    main()
