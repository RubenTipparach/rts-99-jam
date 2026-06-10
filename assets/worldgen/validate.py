#!/usr/bin/env python3
"""Validate every baked map against the skirmish scenario's site contract.

Replicates the client's passability rule (apps/client/src/game.rs
`terrain_blocked`: submerged, or slope over 1.3 height/unit sampled +-4 u,
on the 128-cell grid the sim is fed) directly on the baked `.vxl`, then
checks per world:

  1. spawn apron: at least 85% of passability cells within 40 u of each HQ
     are walkable, dry land;
  2. spawns connected: every HQ shares one walkable region;
  3. majority region: that region covers at least 55% of all walkable cells
     (no base stranded in a minority pocket of the map);
  4. resources reachable: every ore/carbon node touches the spawn region
     (its own cell or an 8-neighbour is walkable and in-region).

Run it after `bake.py` or after editing the `.map`; fix the generator (site
masking, levelling, road grading in densitygen.py), never the checks.

Usage: python3 assets/worldgen/validate.py [keys...]   (default: all worlds)
"""

import math
import os
import sys
from collections import deque

sys.path.insert(0, os.path.dirname(__file__))

import voxel as vox
import worlds as cat
from common import scenario_sites

HALF = 512.0
ISO = 128
MAX_WALK_SLOPE = 1.3
N = 128                      # passability grid fed to the sim
CELL = 2.0 * HALF / N
APRON_R = 40.0               # world units checked around each HQ
APRON_MIN = 0.85             # walkable fraction the apron must reach
REGION_MIN = 0.55            # walkable fraction the spawn region must cover
MAPS_DIR = os.path.join(os.path.dirname(__file__), "..", "maps")


def surface_height(g, x, z):
    dx = (g.bounds[1] - g.bounds[0]) / (g.nx - 1)
    dz = (g.bounds[5] - g.bounds[4]) / (g.nz - 1)
    dy = (g.bounds[3] - g.bounds[2]) / (g.ny - 1)
    i = min(max(int(round((x - g.bounds[0]) / dx)), 0), g.nx - 1)
    k = min(max(int(round((z - g.bounds[4]) / dz)), 0), g.nz - 1)
    for j in range(g.ny - 1, -1, -1):
        d = g.density[g.lin(i, j, k)]
        if d >= ISO:
            y = g.bounds[2] + j * dy
            if j + 1 < g.ny:
                above = g.density[g.lin(i, j + 1, k)]
                t = (ISO - d) / (above - d) if above != d else 0.0
                return y + min(max(t, 0.0), 1.0) * dy
            return y
    return g.bounds[2]


def submerged(g, x, z):
    dx = (g.bounds[1] - g.bounds[0]) / (g.nx - 1)
    dz = (g.bounds[5] - g.bounds[4]) / (g.nz - 1)
    i = min(max(int(round((x - g.bounds[0]) / dx)), 0), g.nx - 1)
    k = min(max(int(round((z - g.bounds[4]) / dz)), 0), g.nz - 1)
    return g.liquid[k * g.nx + i] > 0


def blocked(g, x, z):
    if submerged(g, x, z):
        return True
    s = 4.0
    gx = abs(surface_height(g, x + s, z) - surface_height(g, x - s, z))
    gz = abs(surface_height(g, x, z + s) - surface_height(g, x, z - s))
    return max(gx, gz) / (2.0 * s) > MAX_WALK_SLOPE


def cell_of(wx, wz):
    return (min(max(int((wx + HALF) / CELL), 0), N - 1),
            min(max(int((wz + HALF) / CELL), 0), N - 1))


def centre(i, k):
    return (-HALF + (i + 0.5) * CELL, -HALF + (k + 0.5) * CELL)


def check(key, spawns, resources):
    g = vox.VoxelGrid.load(os.path.join(MAPS_DIR, key + ".vxl"))
    walk = [[not blocked(g, *centre(i, k)) for i in range(N)] for k in range(N)]
    total = sum(row.count(True) for row in walk)
    fails = []

    # 1. spawn aprons
    for n, (sx, sz) in enumerate(spawns):
        r = int(APRON_R / CELL) + 1
        si, sk = cell_of(sx, sz)
        good = bad = 0
        for k in range(max(0, sk - r), min(N, sk + r + 1)):
            for i in range(max(0, si - r), min(N, si + r + 1)):
                cx, cz = centre(i, k)
                if math.hypot(cx - sx, cz - sz) > APRON_R:
                    continue
                if walk[k][i]:
                    good += 1
                else:
                    bad += 1
        frac = good / max(good + bad, 1)
        if frac < APRON_MIN:
            fails.append(f"spawn {n} apron only {frac * 100:.0f}% walkable")

    # 2 + 3. one region for all spawns, covering most of the walkable map
    si, sk = cell_of(*spawns[0])
    seen = [[False] * N for _ in range(N)]
    region = 0
    if walk[sk][si]:
        q = deque([(si, sk)])
        seen[sk][si] = True
        while q:
            i, k = q.popleft()
            region += 1
            for di, dk in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                ni, nk = i + di, k + dk
                if 0 <= ni < N and 0 <= nk < N and not seen[nk][ni] and walk[nk][ni]:
                    seen[nk][ni] = True
                    q.append((ni, nk))
    else:
        fails.append("spawn 0 cell is not walkable")
    for n, (sx, sz) in enumerate(spawns[1:], 1):
        i, k = cell_of(sx, sz)
        if not seen[k][i]:
            fails.append(f"spawn {n} unreachable from spawn 0")
    if total and region / total < REGION_MIN:
        fails.append(
            f"spawn region covers {region * 100 // max(total, 1)}% of walkable "
            f"cells (minority pocket)")

    # 4. every resource node touches the spawn region
    stranded = 0
    for (rx, rz) in resources:
        i, k = cell_of(rx, rz)
        ok = False
        for dk in (-1, 0, 1):
            for di in (-1, 0, 1):
                ni, nk = i + di, k + dk
                if 0 <= ni < N and 0 <= nk < N and seen[nk][ni]:
                    ok = True
        if not ok:
            stranded += 1
    if stranded:
        fails.append(f"{stranded}/{len(resources)} resource nodes unreachable")

    pct = 100 * region / max(total, 1)
    tag = "PASS" if not fails else "FAIL"
    print(f"  {key:10s} {tag}  walkable {100 * total // (N * N)}%  "
          f"spawn region {pct:.1f}% of walkable")
    for f in fails:
        print(f"      - {f}")
    return not fails


def main():
    spawns, resources = scenario_sites()
    keys = sys.argv[1:] or [w["key"] for w in cat.WORLDS]
    bad = 0
    for key in keys:
        path = os.path.join(MAPS_DIR, key + ".vxl")
        if not os.path.exists(path):
            print(f"  {key:10s} SKIP  (not baked)")
            continue
        if not check(key, spawns, resources):
            bad += 1
    if bad:
        print(f"{bad} map(s) failed site validation")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
