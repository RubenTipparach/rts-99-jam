#!/usr/bin/env python3
"""Build a per-world voxel density field that is buildable, not spiky.

Design goals (from the brief):
  - a guaranteed per-world fraction of relatively FLAT, buildable ground,
  - crisp PLATEAUS and CLIFFS rather than high-frequency noise spikes,
  - some genuinely 3D features (caves / overhangs) that a heightmap cannot do.

How: start from a smooth, low-frequency base surface (no fine noise), drop in a
few wide shallow craters (flat floors, not spikes), then TERRACE the surface into
discrete levels. Terracing is the key trick: it turns gentle slopes into flat
plateau tops separated by short cliff risers, which both kills spikes and creates
buildable area. The terrace step is tuned per world to hit its flat target. The
field is then voxelized (with a thin vertical iso-band so the mesh is clean) and
a few caves/arches are carved for 3D interest. A per-column buildable mask marks
the flat tops.
"""

import math

from common import fbm, clamp, Rng
import voxel as vox

HALF = 600
NXZ = 128
NY = 48
YMIN, YMAX = -30.0, 150.0

MAT_LOW, MAT_MID, MAT_HIGH, MAT_ACCENT, MAT_HAZARD = 0, 1, 2, 3, 4

# Per-world buildable target (fraction of map that should be relatively flat).
# Plains/resurfaced worlds are flatter; cratered/chaotic ones rougher.
FLAT_TARGET = {
    "pluto": 0.60, "triton": 0.55, "europa": 0.55, "enceladus": 0.52,
    "titan": 0.50, "moon": 0.45, "mars": 0.45, "ceres": 0.42, "rhea": 0.42,
    "dione": 0.42, "ganymede": 0.40, "io": 0.42, "ariel": 0.40, "titania": 0.38,
    "iapetus": 0.38, "chiron": 0.36, "vesta": 0.34, "oberon": 0.33,
    "umbriel": 0.33, "callisto": 0.32, "miranda": 0.28,
}
CAVES = {
    "miranda": 7, "ariel": 5, "titania": 4, "ganymede": 4, "vesta": 4,
    "europa": 2, "enceladus": 2, "io": 3, "callisto": 4, "oberon": 4,
}


def _base_height(world, amp, freq):
    """Smooth, low-frequency surface height (world units), plus wide craters."""
    t = world["terrain"]
    seed = t["seed"]
    h = [[0.0] * NXZ for _ in range(NXZ)]
    for kk in range(NXZ):
        v = kk / (NXZ - 1)
        for ii in range(NXZ):
            u = ii / (NXZ - 1)
            n = fbm(u * freq, v * freq, seed, octaves=3)        # smooth, no fine noise
            roll = fbm(u * freq * 0.4 + 3, v * freq * 0.4 + 7, seed + 9, 2)
            h[kk][ii] = (n - 0.5) * 2.0 * amp + (roll - 0.5) * amp * 0.5

    # A few wide, shallow craters: flat floors (buildable) and gentle rims, no spikes.
    rng = Rng(seed * 131 + 5)
    n_craters = int(6 * t["crater_density"])
    for _ in range(n_craters):
        cx, cz = rng.uniform(0, NXZ), rng.uniform(0, NXZ)
        rad = rng.uniform(0.08, 0.20) * NXZ
        depth = rad * 0.10
        for kk in range(max(0, int(cz - rad)), min(NXZ, int(cz + rad) + 1)):
            for ii in range(max(0, int(cx - rad)), min(NXZ, int(cx + rad) + 1)):
                d = math.hypot(ii - cx, kk - cz) / rad
                if d <= 1.0:
                    floor = -depth * (1.0 - (d / 0.8) ** 2) if d < 0.8 else 0.0
                    rim = depth * 0.5 * math.exp(-((d - 0.9) / 0.12) ** 2)
                    h[kk][ii] += floor + rim
    return h


def _terrace(h, step):
    out = [[round(h[k][i] / step) * step for i in range(NXZ)] for k in range(NXZ)]
    return out


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


def build(world):
    """Return (VoxelGrid, stats) for `world`."""
    key = world["key"]
    t = world["terrain"]
    target = FLAT_TARGET.get(key, 0.40)
    rough = 1.0 - target
    # Low frequency -> a few big contiguous plateaus (mesas), not contour rings.
    # Rougher worlds get taller relief and a few more tiers (more cliffs).
    amp = min(70.0, t["relief"] * (34.0 + rough * 44.0))
    freq = 1.7 + rough * 2.0
    hbase = _base_height(world, amp, freq)
    span0 = max(1.0, max(max(r) for r in hbase) - min(min(r) for r in hbase))

    # Quantize into a few elevation tiers: fewer tiers -> bigger plateaus (more
    # flat). Pick the tier count whose flat fraction lands closest to target.
    dy = (YMAX - YMIN) / (NY - 1)
    tol = dy * 0.6
    best = None
    for tiers in (2, 3, 4, 5, 6, 8, 10, 12):
        step = max(dy * 1.5, span0 / tiers)
        ht = _terrace(hbase, step)
        f = _flat_fraction(ht, tol)
        if best is None or abs(f - target) < abs(best[2] - target):
            best = (ht, step, f)
    ht, step, flat = best

    # Shift so the lowest terrace sits at 0 (solid fills down to YMIN for caves).
    lo0 = min(min(r) for r in ht)
    ht = [[ht[k][i] - lo0 for i in range(NXZ)] for k in range(NXZ)]

    grid = vox.VoxelGrid(NXZ, NY, NXZ, (-HALF, HALF, YMIN, YMAX, -HALF, HALF))
    lo = 0.0
    hi = max(max(r) for r in ht)
    span = hi or 1.0
    slope_per_unit = 120.0 / dy  # ~1.3-voxel iso band -> clean surface

    # Column material + buildable mask from the terraced surface.
    for k in range(NXZ):
        for i in range(NXZ):
            surf = ht[k][i]
            # local height delta -> slope/cliff classification
            mx = 0.0
            for dk, di in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                nk, ni = k + dk, i + di
                if 0 <= nk < NXZ and 0 <= ni < NXZ:
                    mx = max(mx, abs(ht[nk][ni] - surf))
            en = (surf - lo) / span
            if mx > step * 0.5:
                mat = MAT_HIGH            # cliff face / steep -> rock/ridge
            elif en < 0.28:
                mat = MAT_LOW            # basins
            elif en > 0.74:
                mat = MAT_ACCENT         # high ground accent (rays / ridges)
            else:
                mat = MAT_MID
            buildable = 1 if mx <= tol else 0
            grid.buildable[k * NXZ + i] = buildable

            # Fill the column's density with a thin vertical transition at surf.
            base = i  # x index
            for j in range(NY):
                y = YMIN + j * dy
                d = 128.0 + (surf - y) * slope_per_unit
                grid.density[grid.lin(i, j, k)] = int(clamp(d, 0, 255))
                grid.material[grid.lin(i, j, k)] = mat

    # 3D features: carve caves/arches into cliffy (non-buildable) ground.
    rng = Rng(world["terrain"]["seed"] * 977 + 3)
    holes = CAVES.get(key, 2)
    placed = 0
    attempts = 0
    while placed < holes and attempts < holes * 12:
        attempts += 1
        i = rng.randint(6, NXZ - 7)
        k = rng.randint(6, NXZ - 7)
        if grid.buildable[k * NXZ + i]:
            continue  # keep build space intact
        x, _, z = grid.world(i, 0, k)
        surf = ht[k][i]
        r = rng.uniform(14, 30)
        cy = YMIN + (surf - YMIN) * rng.uniform(0.45, 0.8)
        grid.carve_sphere(x, cy, z, r)
        placed += 1

    flat_final = sum(grid.buildable) / (NXZ * NXZ)
    grid.flat_permil = int(round(flat_final * 1000))
    stats = dict(flat=flat_final, step=step, caves=placed, lo=lo, hi=hi)
    return grid, stats


if __name__ == "__main__":
    import sys
    import worlds as cat
    keys = sys.argv[1:] or [w["key"] for w in cat.WORLDS]
    for key in keys:
        g, s = build(cat.by_key(key))
        print(f"{key:10s} flat={s['flat']*100:4.1f}%  step={s['step']:5.1f}  "
              f"caves={s['caves']}  hrange=[{s['lo']:.0f},{s['hi']:.0f}]")
