#!/usr/bin/env python3
"""Marching cubes over a voxel density grid.

Polygonizes the iso-surface of a `VoxelGrid` into a triangle soup. Each cube is
decomposed into six tetrahedra sharing the 0-6 space diagonal and each tetra is
contoured directly: this is the robust, watertight tetrahedral variant of
marching cubes (no 256-entry lookup table to mis-transcribe, no ambiguous-face
cases). Per-vertex normals come from the density gradient, so shading is smooth
and winding order is irrelevant (the previewer z-buffers; the engine can cull or
not as it likes).

Returns flat lists suitable for both the Python previewer and the Rust loader:
    verts:   [(x,y,z), ...]
    normals: [(nx,ny,nz), ...]   (unit, outward = toward air)
    tris:    [(i0,i1,i2), ...]   indices into verts
    tmat:    [material_id, ...]  one per triangle
"""

import math

from voxel import ISO

# Cube corner offsets (i=x, j=y up, k=z), matching voxel.lin ordering.
_CORNERS = [(0, 0, 0), (1, 0, 0), (1, 1, 0), (0, 1, 0),
            (0, 0, 1), (1, 0, 1), (1, 1, 1), (0, 1, 1)]
# Six tetrahedra tiling the cube around the 0-6 diagonal.
_TETS = [(0, 1, 2, 6), (0, 2, 3, 6), (0, 3, 7, 6),
         (0, 7, 4, 6), (0, 4, 5, 6), (0, 5, 1, 6)]


def polygonize(grid, step=1):
    """March `grid` (optionally decimated by integer `step`) -> mesh lists."""
    nx, ny, nz = grid.nx, grid.ny, grid.nz
    dens = grid.density
    iso = ISO
    verts, normals, tris, tmat = [], [], [], []
    vcache = {}

    def lin(i, j, k):
        return (j * nz + k) * nx + i

    def vertex(pa, va, pb, vb):
        """Interpolated surface vertex on edge a-b, normal from the gradient."""
        t = 0.5 if vb == va else (iso - va) / (vb - va)
        t = 0.0 if t < 0 else 1.0 if t > 1 else t
        x = pa[0] + (pb[0] - pa[0]) * t
        y = pa[1] + (pb[1] - pa[1]) * t
        z = pa[2] + (pb[2] - pa[2]) * t
        key = (round(x, 2), round(y, 2), round(z, 2))
        vi = vcache.get(key)
        if vi is None:
            gx, gy, gz = grid.gradient(x, y, z)
            ln = math.sqrt(gx * gx + gy * gy + gz * gz) or 1.0
            vi = len(verts)
            verts.append((x, y, z))
            normals.append((-gx / ln, -gy / ln, -gz / ln))
            vcache[key] = vi
        return vi

    rng = range
    for k in rng(0, nz - step, step):
        for j in rng(0, ny - step, step):
            base = (j * nz + k) * nx
            for i in rng(0, nx - step, step):
                # gather 8 corner densities; early-out on fully solid/air cubes
                cv = [
                    dens[lin(i + dx * step, j + dy * step, k + dz * step)]
                    for (dx, dy, dz) in _CORNERS
                ]
                mn = min(cv)
                mx = max(cv)
                if mn >= iso or mx < iso:
                    continue
                cp = [grid.world(i + dx * step, j + dy * step, k + dz * step)
                      for (dx, dy, dz) in _CORNERS]
                mat = grid.material[base + i]
                for tet in _TETS:
                    solids = [c for c in tet if cv[c] >= iso]
                    airs = [c for c in tet if cv[c] < iso]
                    ns = len(solids)
                    if ns == 0 or ns == 4:
                        continue
                    if ns == 1 or ns == 3:
                        odd = solids[0] if ns == 1 else airs[0]
                        rest = [c for c in tet if c != odd]
                        a = vertex(cp[odd], cv[odd], cp[rest[0]], cv[rest[0]])
                        b = vertex(cp[odd], cv[odd], cp[rest[1]], cv[rest[1]])
                        c = vertex(cp[odd], cv[odd], cp[rest[2]], cv[rest[2]])
                        tris.append((a, b, c))
                        tmat.append(mat)
                    else:  # 2 solid, 2 air -> quad (two triangles)
                        s0, s1 = solids
                        a0, a1 = airs
                        A = vertex(cp[s0], cv[s0], cp[a0], cv[a0])
                        B = vertex(cp[s0], cv[s0], cp[a1], cv[a1])
                        C = vertex(cp[s1], cv[s1], cp[a1], cv[a1])
                        D = vertex(cp[s1], cv[s1], cp[a0], cv[a0])
                        tris.append((A, B, C))
                        tris.append((A, C, D))
                        tmat.append(mat)
                        tmat.append(mat)
    return verts, normals, tris, tmat


if __name__ == "__main__":
    # Self-test: contour a sphere and report a sane triangle count.
    from voxel import VoxelGrid
    g = VoxelGrid(40, 40, 40, (-20, 20, -20, 20, -20, 20))
    for j in range(g.ny):
        for k in range(g.nz):
            for i in range(g.nx):
                x, y, z = g.world(i, j, k)
                d = 255 if (x * x + y * y + z * z) <= 12 * 12 else 0
                g.density[g.lin(i, j, k)] = d
    v, n, t, m = polygonize(g)
    print(f"sphere -> {len(v)} verts, {len(t)} tris")
    assert len(t) > 200, "sphere mesh suspiciously small"
    # all normals roughly unit length
    bad = sum(1 for nx, ny, nz in n if abs((nx * nx + ny * ny + nz * nz) ** 0.5 - 1.0) > 0.01)
    print(f"  non-unit normals: {bad}")
    print("marching cubes ok")
