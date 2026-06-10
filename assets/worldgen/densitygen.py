#!/usr/bin/env python3
"""Build a per-world voxel density field: buildable, not spiky, with the right
landforms for each body.

Base shaping (all worlds): a smooth low-frequency surface terraced into a few big
elevation tiers, so gentle slopes become flat buildable plateau tops separated by
short cliff risers (kills noise spikes, guarantees build space). Then each world
gets the features that actually define it, driven by how it is eroded:

  - Airless rocky/dusty bodies (no erosion): LOTS of impact cratering
    (Moon, Ceres, Vesta, Callisto, Rhea, Dione, Iapetus, the Uranian moons, ...).
  - Ice-resurfaced: few craters, plus their signature -
    Europa long deep fissures, Enceladus mini geysers, Triton cantaloupe.
  - Volcanic (Io): few craters, GIANT volcanoes (cones + summit calderas + lava).
  - Atmospheric: Mars is fairly plain with one big central canyon; Titan has
    oceans of liquid methane; Earth has oceans, lakes and rivers.

Genuinely 3D features (caves/overhangs) are carved into steep ground where it
won't eat build space. A per-column buildable mask marks the flat, dry tops.
"""

import math

from common import fbm, clamp, Rng
import voxel as vox

# World is 1024 x 1024 units (HALF = 512), and the XZ grid is 257 samples = 256
# cells, so dx = dz = 1024 / 256 = exactly 4.0 units. That divides cleanly into
# the sim's integer logical units and lines up 1:1 with the 256-cell fog grid
# (apps/client gfx::FOW_RES). Keep these in step with terrain::HALF.
HALF = 512
NXZ = 257
NY = 128
YMIN, YMAX = -40.0, 150.0

# World-space (resolution-independent) tuning, so the grid can be re-sampled at a
# higher NXZ/NY without terrain getting "stricter": a flat-enough-to-build column
# and the surface-skin thickness are physical sizes, not voxel counts.
FLAT_TOL = 2.4   # max neighbour height delta (world units) still counted buildable
SKIN_MIN, SKIN_VAR = 4.8, 6.5  # surface skin thickness range (world units)

MAT_LOW, MAT_MID, MAT_HIGH, MAT_ACCENT, MAT_HAZARD = 0, 1, 2, 3, 4

# Archetypes that are ice all the way down: a cut (crater wall, fissure, canyon,
# cave) exposes clean/bright ice (MAT_ACCENT) under the dusty/grooved skin. Every
# other body exposes bedrock (MAT_HIGH). This is the per-voxel subsurface, which
# makes material a genuine 3D property instead of a per-column height lookup.
ICE_ARCH = {
    "dirty_ice", "grooved_ice", "bright_ice", "europa_ice", "triton_ice",
    "pluto_tholin",
}

# Per-world buildable target (fraction of map that should be relatively flat).
FLAT_TARGET = {
    "earth": 0.50, "pluto": 0.60, "triton": 0.55, "europa": 0.58, "enceladus": 0.55,
    "titan": 0.55, "mars": 0.60, "moon": 0.42, "ceres": 0.40, "rhea": 0.40,
    "dione": 0.40, "ganymede": 0.42, "io": 0.46, "ariel": 0.42, "titania": 0.36,
    "iapetus": 0.36, "chiron": 0.36, "vesta": 0.34, "oberon": 0.33,
    "umbriel": 0.34, "callisto": 0.32, "miranda": 0.30,
}
CAVES = {
    "miranda": 7, "ariel": 5, "titania": 4, "ganymede": 4, "vesta": 3,
    "europa": 1, "enceladus": 1, "io": 2, "callisto": 3, "oberon": 3, "earth": 1,
}

# How much of the hard terraced "wedding cake" to keep (1 = full plateaus/cliffs,
# 0 = fully smooth rolling). Lower values + the smoothing passes below trade hard
# cliffs for rounded slopes and more varied terrain.
TERRACE_MIX = {
    "io": 0.0, "earth": 0.4, "pluto": 0.45, "triton": 0.5, "titan": 0.55,
    "miranda": 0.72, "ariel": 0.66, "ganymede": 0.6,
}
# Box-blur passes over the height field before voxelizing: rounds the cliffs and
# ridges the marching-cubes surface would otherwise show as hard facets.
SMOOTH = {
    "io": 3, "earth": 2, "pluto": 2, "triton": 2, "titan": 1, "miranda": 1,
}


def _base_height(world, amp, freq):
    """Smooth, low-frequency surface height in world units (no fine noise)."""
    seed = world["terrain"]["seed"]
    h = [[0.0] * NXZ for _ in range(NXZ)]
    for kk in range(NXZ):
        v = kk / (NXZ - 1)
        for ii in range(NXZ):
            u = ii / (NXZ - 1)
            n = fbm(u * freq, v * freq, seed, octaves=3)
            roll = fbm(u * freq * 0.4 + 3, v * freq * 0.4 + 7, seed + 9, 2)
            h[kk][ii] = (n - 0.5) * 2.0 * amp + (roll - 0.5) * amp * 0.5
    return h


def _terrace(h, step):
    return [[round(h[k][i] / step) * step for i in range(NXZ)] for k in range(NXZ)]


def _smooth(h, passes):
    """Separable 3-tap box blur over the height field (edge-clamped). Softens the
    cliffs/ridges marching cubes would otherwise render as hard facets."""
    for _ in range(passes):
        tmp = [[0.0] * NXZ for _ in range(NXZ)]
        for k in range(NXZ):
            row = h[k]
            for i in range(NXZ):
                s = row[i]
                c = 1
                if i > 0:
                    s += row[i - 1]; c += 1
                if i < NXZ - 1:
                    s += row[i + 1]; c += 1
                tmp[k][i] = s / c
        for k in range(NXZ):
            for i in range(NXZ):
                s = tmp[k][i]
                c = 1
                if k > 0:
                    s += tmp[k - 1][i]; c += 1
                if k < NXZ - 1:
                    s += tmp[k + 1][i]; c += 1
                h[k][i] = s / c
    return h


def _flat_fraction(ht, tol):
    flat = 0
    for k in range(NXZ):
        for i in range(NXZ):
            v = ht[k][i]
            ok = True
            for dk, di in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                nk, ni = k + dk, i + di
                if 0 <= nk < NXZ and 0 <= ni < NXZ and abs(ht[nk][ni] - v) > tol:
                    ok = False
                    break
            if ok:
                flat += 1
    return flat / (NXZ * NXZ)


# --------------------------------------------------------------------------- #
# erosion-driven features (operate on the terraced height field `ht`)
# --------------------------------------------------------------------------- #
def _crater_count(world):
    """Impact craters scale with how *un*-eroded (airless, ancient) a world is."""
    a = world["archetype"]
    k = world["key"]
    if k == "mars":
        return 16            # eroded, but still visibly cratered
    if a in ("regolith_grey", "regolith_dark", "dirty_ice"):
        return 70            # airless rock / ancient dirty ice: saturated craters
    if a == "grooved_ice":
        return 24            # partially resurfaced
    if a == "pluto_tholin":
        return 14
    return 3                 # ice/volcanic/water resurfaced: nearly crater-free


def _craters(ht, mat, seed, count):
    rng = Rng(seed * 131 + 5)
    for _ in range(count):
        cx, cz = rng.uniform(0, NXZ), rng.uniform(0, NXZ)
        big = rng.rand() < 0.16
        rad = rng.uniform(0.06, 0.12) * NXZ if big else rng.uniform(0.02, 0.06) * NXZ
        depth = rad * 0.62                            # deeper -> more prominent
        i0, i1 = max(0, int(cx - rad * 1.6)), min(NXZ, int(cx + rad * 1.6) + 1)
        k0, k1 = max(0, int(cz - rad * 1.6)), min(NXZ, int(cz + rad * 1.6) + 1)
        for kk in range(k0, k1):
            for ii in range(i0, i1):
                d = math.hypot(ii - cx, kk - cz) / rad
                if d > 1.5:
                    continue
                bowl = -depth * (1.0 - d * d) if d < 1.0 else 0.0
                rim = depth * 0.55 * math.exp(-((d - 1.0) / 0.18) ** 2)
                ht[kk][ii] += bowl + rim
                if d < 0.55:
                    mat[kk][ii] = MAT_LOW                 # dark crater floor
                elif 1.0 < d < 1.35 and big:
                    mat[kk][ii] = MAT_ACCENT              # bright ejecta rays


def _fissures(ht, mat, seed, n, depth, halfw):
    """Europa-style long, deep, narrow fissures (lineae) carved across the map.

    Accumulate a max-depth cut map (so overlapping steps do not stack into a
    bottomless trench), then apply it once.
    """
    rng = Rng(seed * 17 + 1)
    cut = [[0.0] * NXZ for _ in range(NXZ)]
    for _ in range(n):
        x, z = rng.uniform(0, NXZ), rng.uniform(0, NXZ)
        ang = rng.uniform(0, 2 * math.pi)
        length = int(NXZ * rng.uniform(0.8, 1.4))
        for _ in range(length):
            x += math.cos(ang)
            z += math.sin(ang)
            ang += (rng.rand() - 0.5) * 0.18
            if not (0 <= x < NXZ and 0 <= z < NXZ):
                break
            for dk in range(-halfw, halfw + 1):
                for di in range(-halfw, halfw + 1):
                    ii, kk = int(x + di), int(z + dk)
                    if 0 <= ii < NXZ and 0 <= kk < NXZ:
                        dd = math.hypot(di, dk) / (halfw + 0.5)
                        if dd < 1.0:
                            cut[kk][ii] = max(cut[kk][ii], depth * (1.0 - dd * dd))
                            mat[kk][ii] = MAT_ACCENT
    for k in range(NXZ):
        for i in range(NXZ):
            ht[k][i] -= cut[k][i]


def _canyon(ht, mat, seed, depth, halfw):
    """One big meandering canyon across the middle (Mars: Valles Marineris)."""
    for ii in range(NXZ):
        u = ii / (NXZ - 1)
        cz = NXZ * 0.5 + math.sin(u * math.pi * 1.6) * NXZ * 0.06 \
            + (fbm(u * 3.0, 0.5, seed + 4, 2) - 0.5) * NXZ * 0.14
        for kk in range(NXZ):
            dd = abs(kk - cz) / halfw
            if dd < 1.0:
                ht[kk][ii] -= depth * (1.0 - dd * dd)
                mat[kk][ii] = MAT_HIGH if dd > 0.6 else MAT_LOW


def _cones(ht, mat, seed, n, height, base_r, kind, region=None):
    """Build `n` volcanic cones with a summit caldera + hazard material.

    One builder for all of them, scaled by `height`/`base_r`:
      - Io: giant volcanoes (tall, wide), `kind="volcano"` (lava);
      - Titan: towering cryovolcanoes, `kind="cryovolcano"`;
      - Enceladus / Triton: small volcano-like geysers, `kind="geyser"`.
    `region="south"` clusters them toward the south pole (tiger stripes).
    """
    rng = Rng(seed * 23 + (len(kind) * 13 + 4))
    vents = []
    for _ in range(n):
        cx = rng.uniform(base_r, NXZ - base_r)
        if region == "south":
            cz = rng.uniform(NXZ * 0.52, NXZ - base_r)
        else:
            cz = rng.uniform(base_r, NXZ - base_r)
        i0, i1 = int(cx - base_r), int(cx + base_r) + 1
        k0, k1 = int(cz - base_r), int(cz + base_r) + 1
        for kk in range(max(0, k0), min(NXZ, k1)):
            for ii in range(max(0, i0), min(NXZ, i1)):
                d = math.hypot(ii - cx, kk - cz) / base_r
                if d >= 1.0:
                    continue
                ht[kk][ii] += height * (1.0 - d) ** 1.6
                if d < 0.18:
                    ht[kk][ii] -= height * 0.3           # summit caldera
                    mat[kk][ii] = MAT_HAZARD
                elif d < 0.26:
                    mat[kk][ii] = MAT_HAZARD
        vents.append((int(cx), int(cz), kind))
    return vents


# The fixed skirmish spawn sites in world (x, z) - the three mains in
# assets/maps/crossfire_basin.map. Every main must start on dry land, so wet
# worlds dome the ground above sea level here and never flood these discs.
SPAWNS = ((0.0, 210.0), (-150.0, -190.0), (150.0, -190.0))
SPAWN_R = NXZ * 0.075  # ~77 world units around each base


def _spawn_dist(i, k):
    best = 1e9
    for sx, sz in SPAWNS:
        ci = (sx + HALF) / (2.0 * HALF) * (NXZ - 1)
        ck = (sz + HALF) / (2.0 * HALF) * (NXZ - 1)
        best = min(best, math.hypot(i - ci, k - ck))
    return best


def _liquid(grid, ht, world, dy):
    """Fill oceans (Titan methane, Earth seas), Earth lakes and rivers.

    Writes grid.liquid (per-column j+1 of the liquid surface) and grid.sea_level.
    Returns the set of liquid columns so they can be excluded from buildable.
    """
    key = world["key"]
    wet = set()
    if key not in ("titan", "earth"):
        return wet

    flat = [ht[k][i] for k in range(NXZ) for i in range(NXZ)]
    flat.sort()
    frac = 0.46 if key == "earth" else 0.42
    sea = flat[int(len(flat) * frac)]
    grid.sea_level = YMIN + 0.0  # set below after mapping to world y
    sea_y = sea  # ht is already in world-y units after shift

    def set_liquid(i, k, surf_y):
        if _spawn_dist(i, k) < SPAWN_R * 0.85:
            return  # spawn discs stay dry (lakes and rivers included)
        j = int(round((surf_y - YMIN) / dy))
        j = max(0, min(NY - 1, j))
        grid.liquid[k * NXZ + i] = j + 1
        wet.add((i, k))

    # Dome every spawn disc above the sea: the centre is guaranteed dry land,
    # feathering back to the natural terrain at the disc's edge.
    for k in range(NXZ):
        for i in range(NXZ):
            d = _spawn_dist(i, k) / SPAWN_R
            if d >= 1.0:
                continue
            need = sea_y + 6.0
            if d < 0.6:
                ht[k][i] = max(ht[k][i], need)
            elif ht[k][i] < need:
                t = (1.0 - d) / 0.4
                ht[k][i] += (need - ht[k][i]) * t * t

    # oceans / seas
    for k in range(NXZ):
        for i in range(NXZ):
            if ht[k][i] < sea_y:
                set_liquid(i, k, sea_y)
    grid.sea_level = sea_y

    if key == "earth":
        rng = Rng(world["terrain"]["seed"] * 41 + 9)
        # a few inland lakes: fill a small basin above sea level
        for _ in range(5):
            ci = rng.randint(16, NXZ - 17)
            ck = rng.randint(16, NXZ - 17)
            if (ci, ck) in wet:
                continue
            level = ht[ck][ci] + rng.uniform(3.0, 7.0)
            r = rng.uniform(6, 12)
            for kk in range(max(0, int(ck - r)), min(NXZ, int(ck + r) + 1)):
                for ii in range(max(0, int(ci - r)), min(NXZ, int(ci + r) + 1)):
                    if math.hypot(ii - ci, kk - ck) <= r and ht[kk][ii] < level:
                        set_liquid(ii, kk, level)
        # rivers: greedy downhill walk from highlands to water
        for _ in range(7):
            x = rng.randint(8, NXZ - 9)
            z = rng.randint(8, NXZ - 9)
            for _ in range(NXZ):
                if (x, z) in wet:
                    break
                # step to the lowest 4-neighbour
                best = None
                for dk, di in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                    nx_, nz_ = x + di, z + dk
                    if 0 <= nx_ < NXZ and 0 <= nz_ < NXZ:
                        if best is None or ht[nz_][nx_] < ht[best[1]][best[0]]:
                            best = (nx_, nz_)
                if best is None or ht[best[1]][best[0]] >= ht[z][x]:
                    break  # local minimum (let it pond, stop)
                set_liquid(x, z, ht[z][x] - 0.5)
                x, z = best
    return wet


# --------------------------------------------------------------------------- #
def build(world):
    """Return (VoxelGrid, stats) for `world`."""
    key = world["key"]
    t = world["terrain"]
    target = FLAT_TARGET.get(key, 0.40)
    rough = 1.0 - target
    dy = (YMAX - YMIN) / (NY - 1)
    tol = FLAT_TOL

    # Mostly-flat worlds whose relief comes from discrete features, not terraced
    # mesas: airless rock/ice with no erosion (Moon, Ceres, Vesta, Callisto,
    # Rhea, Dione, Iapetus, the Uranian moons, Chiron) where craters do the work,
    # and Io (flat sulfur plains studded with giant volcanoes).
    crater_dominated = world["archetype"] in ("regolith_grey", "regolith_dark", "dirty_ice")
    flat_base = crater_dominated or key == "io"
    if flat_base:
        amp = min(20.0, 8.0 + t["relief"] * 9.0)
        freq = 1.55
        plain_target = 0.74
        tiers_try = (1, 2, 3, 4)
    else:
        amp = min(70.0, t["relief"] * (34.0 + rough * 44.0))
        if key == "mars":
            amp *= 0.55       # Mars is fairly plain; the canyon is its feature
        freq = 1.7 + rough * 2.0
        plain_target = target
        tiers_try = (2, 3, 4, 5, 6, 8, 10, 12)

    hbase = _base_height(world, amp, freq)
    span0 = max(1.0, max(max(r) for r in hbase) - min(min(r) for r in hbase))
    best = None
    for tiers in tiers_try:
        step = max(dy * 1.5, span0 / tiers)
        ht = _terrace(hbase, step)
        f = _flat_fraction(ht, tol)
        if best is None or abs(f - plain_target) < abs(best[2] - plain_target):
            best = (ht, step, f)
    ht, step, _ = best

    # Variability + smoothing: blend the hard terraces back toward the smooth
    # base (so terrain is a mix of plateaus and rolling slopes, not a uniform
    # wedding cake), then box-blur to round the cliffs/ridges. Craters and other
    # features are added AFTER this, so they stay crisp and prominent.
    mix = TERRACE_MIX.get(key, 0.58)
    ht = [[hbase[k][i] * (1.0 - mix) + ht[k][i] * mix for i in range(NXZ)]
          for k in range(NXZ)]
    ht = _smooth(ht, SMOOTH.get(key, 1))

    # Material overrides + vents collected by the feature passes.
    mat = [[None] * NXZ for _ in range(NXZ)]
    vents = []

    _craters(ht, mat, t["seed"], _crater_count(world))
    if key == "europa":
        _fissures(ht, mat, t["seed"], 7, depth=16.0, halfw=2)
    if key == "ariel":
        _fissures(ht, mat, t["seed"], 4, depth=12.0, halfw=2)
    if key == "mars":
        _canyon(ht, mat, t["seed"], depth=22.0, halfw=NXZ * 0.07)
    if key == "io":
        vents += _cones(ht, mat, t["seed"], 4, height=66.0, base_r=NXZ * 0.18, kind="volcano")
    if key == "enceladus":
        vents += _cones(ht, mat, t["seed"], 9, height=13.0, base_r=NXZ * 0.05,
                        kind="geyser", region="south")
    if key == "triton":
        vents += _cones(ht, mat, t["seed"], 6, height=11.0, base_r=NXZ * 0.05, kind="geyser")

    # Shift so the lowest point sits at 0, then voxelize.
    lo0 = min(min(r) for r in ht)
    ht = [[ht[k][i] - lo0 for i in range(NXZ)] for k in range(NXZ)]

    # Earth is an island: sink the map rim well below the coming sea level
    # (taken as a height percentile in _liquid), so open water rings the
    # landmass instead of seas pooling at random. Clamped above the grid
    # floor so the ocean keeps a solid bed.
    if key == "earth":
        for k in range(NXZ):
            v = k / (NXZ - 1) - 0.5
            for i in range(NXZ):
                u = i / (NXZ - 1) - 0.5
                r = 2.0 * math.hypot(u, v)  # 0 centre, 1 at the edge midpoints
                f = min(1.0, max(0.0, (r - 0.82) / 0.30))
                ht[k][i] = max(-34.0, ht[k][i] - 55.0 * f * f)

    grid = vox.VoxelGrid(NXZ, NY, NXZ, (-HALF, HALF, YMIN, YMAX, -HALF, HALF))
    hi = max(max(r) for r in ht)
    span = hi or 1.0
    slope_per_unit = 120.0 / dy

    wet = _liquid(grid, ht, world, dy)

    # Subsurface material: what an exposed face / crater wall / cave reveals under
    # the thin surface skin. This is what makes texture a genuine 3D property
    # instead of a height lookup: ice bodies show clean ice underneath, everything
    # else shows bedrock. (No format change: the .vxl already stores material per
    # voxel and the renderer already textures from it.)
    ice_under = world["archetype"] in ICE_ARCH
    sub_m = MAT_ACCENT if ice_under else MAT_HIGH
    sk = t["seed"]

    for k in range(NXZ):
        v = k / (NXZ - 1)
        for i in range(NXZ):
            u = i / (NXZ - 1)
            surf = ht[k][i]
            mx = 0.0
            for dk, di in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                nk, ni = k + dk, i + di
                if 0 <= nk < NXZ and 0 <= ni < NXZ:
                    mx = max(mx, abs(ht[nk][ni] - surf))
            en = surf / span
            override = mat[k][i]
            if override is not None:
                skin_m = override
            elif mx > step * 0.5:
                skin_m = MAT_HIGH
            elif en < 0.28:
                skin_m = MAT_LOW
            elif en > 0.74:
                skin_m = MAT_ACCENT
            else:
                skin_m = MAT_MID
            # Height-independent mottle: break up flat plains with x/z-position
            # noise so a plateau is not one uniform tile (skin only, never over a
            # feature stamp).
            if override is None and mx <= step * 0.5 and skin_m == MAT_MID:
                mott = fbm(u * 4.5 + 11.0, v * 4.5 + 4.0, sk + 7, 3)
                if mott > 0.66:
                    skin_m = MAT_LOW
                elif mott < 0.30:
                    skin_m = sub_m
            # Skin thickness wobbles in x/z so the skin->subsurface edge on a face
            # is irregular, not a perfectly level band.
            wob = fbm(u * 3.0 + 2.0, v * 3.0 + 9.0, sk + 13, 2)
            skin = SKIN_MIN + SKIN_VAR * wob
            # buildable: flat, and dry land (not under ocean/lake/river)
            grid.buildable[k * NXZ + i] = 1 if (mx <= tol and (i, k) not in wet) else 0
            for j in range(NY):
                y = YMIN + j * dy
                d = 128.0 + (surf - y) * slope_per_unit
                grid.density[grid.lin(i, j, k)] = int(clamp(d, 0, 255))
                grid.material[grid.lin(i, j, k)] = skin_m if (surf - y) <= skin else sub_m

    # 3D caves/arches into steep, dry, non-buildable ground.
    rng = Rng(t["seed"] * 977 + 3)
    holes = CAVES.get(key, 2)
    placed = 0
    attempts = 0
    while placed < holes and attempts < holes * 12:
        attempts += 1
        i = rng.randint(6, NXZ - 7)
        k = rng.randint(6, NXZ - 7)
        if grid.buildable[k * NXZ + i] or (i, k) in wet:
            continue
        x, _, z = grid.world(i, 0, k)
        cy = YMIN + (ht[k][i] - YMIN) * rng.uniform(0.45, 0.8)
        grid.carve_sphere(x, cy, z, rng.uniform(14, 28))
        placed += 1

    flat_final = sum(grid.buildable) / (NXZ * NXZ)
    grid.flat_permil = int(round(flat_final * 1000))
    stats = dict(flat=flat_final, step=step, caves=placed, hi=hi,
                 vents=vents, wet=len(wet), sea=grid.sea_level)
    return grid, stats


if __name__ == "__main__":
    import sys
    import worlds as cat
    keys = [a for a in sys.argv[1:] if not a.startswith("-")] or [w["key"] for w in cat.WORLDS]
    for key in keys:
        g, s = build(cat.by_key(key))
        liq = f" wet={s['wet']}" if s["wet"] else ""
        vent = f" vents={len(s['vents'])}" if s["vents"] else ""
        print(f"{key:10s} flat={s['flat']*100:4.1f}%  step={s['step']:4.1f}  "
              f"caves={s['caves']}  hi={s['hi']:3.0f}{liq}{vent}")
