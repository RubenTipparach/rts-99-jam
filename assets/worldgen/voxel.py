#!/usr/bin/env python3
"""Voxel / density-grid maps: the static, editable terrain format.

A map is a 3D scalar *density* field on a regular grid (plus a per-voxel material
id). The surface is the iso-contour at `ISO`; voxels with density >= ISO are
solid ground, below it is air. Storing a full 3D field (instead of a 2D
heightmap) is what lets terrain have crisp vertical cliffs, plateaus, overhangs
and caves without the high-frequency-noise spikes a heightmap suffers from. The
mesh is produced by marching cubes (`marching_cubes.py`).

These are the baked, hand-editable map files. The on-disk format `.vxl` is a tiny
documented container:

    magic   "VXL1"                       4 bytes
    nx ny nz                             3 x uint16 (grid dims; y is up)
    x0 x1 y0 y1 z0 z1                    6 x int16  (world-space bounds)
    flat_permil                          uint16     (measured buildable fraction * 1000)
    reserved                             uint16
    payload = zlib( density[nx*ny*nz] + material[nx*ny*nz] + buildable[nx*nz] )
              density, material: uint8;  buildable: uint8 column mask (0/1)

`density`/`material` are indexed `lin(i,j,k) = (j*nz + k)*nx + i`. Editing later
is supported by the ops on `VoxelGrid` (raise/lower/flatten/carve) and the small
CLI at the bottom; everything is presentation-side (no sim determinism here).
"""

import os
import struct
import zlib

ISO = 128  # surface iso-level; density is uint8, >=ISO is solid


class VoxelGrid:
    def __init__(self, nx, ny, nz, bounds):
        self.nx, self.ny, self.nz = nx, ny, nz
        # bounds = (x0, x1, y0, y1, z0, z1) in world units
        self.bounds = tuple(bounds)
        self.density = bytearray(nx * ny * nz)      # 0 = deep air, 255 = deep rock
        self.material = bytearray(nx * ny * nz)      # palette slot id (see densitygen)
        self.buildable = bytearray(nx * nz)          # per-column flat/buildable mask
        self.flat_permil = 0

    # --- indexing & world<->grid mapping ---
    def lin(self, i, j, k):
        return (j * self.nz + k) * self.nx + i

    def dx(self):
        x0, x1, _, _, _, _ = self.bounds
        return (x1 - x0) / (self.nx - 1)

    def dy(self):
        _, _, y0, y1, _, _ = self.bounds
        return (y1 - y0) / (self.ny - 1)

    def dz(self):
        _, _, _, _, z0, z1 = self.bounds
        return (z1 - z0) / (self.nz - 1)

    def world(self, i, j, k):
        x0, _, y0, _, z0, _ = self.bounds
        return (x0 + i * self.dx(), y0 + j * self.dy(), z0 + k * self.dz())

    # --- sampling (trilinear) for normals / mesh-independent queries ---
    def at(self, i, j, k):
        i = 0 if i < 0 else self.nx - 1 if i >= self.nx else i
        j = 0 if j < 0 else self.ny - 1 if j >= self.ny else j
        k = 0 if k < 0 else self.nz - 1 if k >= self.nz else k
        return self.density[self.lin(i, j, k)]

    def sample(self, x, y, z):
        x0, _, y0, _, z0, _ = self.bounds
        fx = (x - x0) / self.dx()
        fy = (y - y0) / self.dy()
        fz = (z - z0) / self.dz()
        i, j, k = int(fx), int(fy), int(fz)
        tx, ty, tz = fx - i, fy - j, fz - k

        def c(di, dj, dk):
            return self.at(i + di, j + dj, k + dk)

        c00 = c(0, 0, 0) * (1 - tx) + c(1, 0, 0) * tx
        c10 = c(0, 1, 0) * (1 - tx) + c(1, 1, 0) * tx
        c01 = c(0, 0, 1) * (1 - tx) + c(1, 0, 1) * tx
        c11 = c(0, 1, 1) * (1 - tx) + c(1, 1, 1) * tx
        c0 = c00 * (1 - ty) + c10 * ty
        c1 = c01 * (1 - ty) + c11 * ty
        return c0 * (1 - tz) + c1 * tz

    def gradient(self, x, y, z):
        e = min(self.dx(), self.dy(), self.dz()) * 0.75
        gx = self.sample(x + e, y, z) - self.sample(x - e, y, z)
        gy = self.sample(x, y + e, z) - self.sample(x, y - e, z)
        gz = self.sample(x, y, z + e) - self.sample(x, y, z - e)
        return (gx, gy, gz)

    # --- editing ops ("go in and edit later") ---
    def fill_box(self, x0, x1, y0, y1, z0, z1, value):
        for j in range(self.ny):
            wy = self.world(0, j, 0)[1]
            if wy < y0 or wy > y1:
                continue
            for k in range(self.nz):
                wz = self.world(0, 0, k)[2]
                if wz < z0 or wz > z1:
                    continue
                for i in range(self.nx):
                    wx = self.world(i, 0, 0)[0]
                    if x0 <= wx <= x1:
                        self.density[self.lin(i, j, k)] = value

    def carve_sphere(self, cx, cy, cz, r, solid=False):
        """Carve (air) or add (solid) a sphere: the simplest overhang/cave tool."""
        val = 255 if solid else 0
        x0, _, y0, _, z0, _ = self.bounds
        dx, dy, dz = self.dx(), self.dy(), self.dz()
        i0 = max(0, int((cx - r - x0) / dx))
        i1 = min(self.nx - 1, int((cx + r - x0) / dx) + 1)
        j0 = max(0, int((cy - r - y0) / dy))
        j1 = min(self.ny - 1, int((cy + r - y0) / dy) + 1)
        k0 = max(0, int((cz - r - z0) / dz))
        k1 = min(self.nz - 1, int((cz + r - z0) / dz) + 1)
        r2 = r * r
        for j in range(j0, j1 + 1):
            wy = y0 + j * dy
            for k in range(k0, k1 + 1):
                wz = z0 + k * dz
                for i in range(i0, i1 + 1):
                    wx = x0 + i * dx
                    if (wx - cx) ** 2 + (wy - cy) ** 2 + (wz - cz) ** 2 <= r2:
                        self.density[self.lin(i, j, k)] = val

    # --- io ---
    def save(self, path):
        os.makedirs(os.path.dirname(path), exist_ok=True)
        x0, x1, y0, y1, z0, z1 = (int(round(b)) for b in self.bounds)
        head = b"VXL1" + struct.pack(
            ">HHHhhhhhhHH", self.nx, self.ny, self.nz,
            x0, x1, y0, y1, z0, z1, self.flat_permil, 0,
        )
        payload = zlib.compress(bytes(self.density) + bytes(self.material)
                                + bytes(self.buildable), 9)
        with open(path, "wb") as f:
            f.write(head + payload)

    @staticmethod
    def load(path):
        with open(path, "rb") as f:
            data = f.read()
        assert data[:4] == b"VXL1", "not a VXL1 file"
        nx, ny, nz, x0, x1, y0, y1, z0, z1, flat, _ = struct.unpack(">HHHhhhhhhHH", data[4:26])
        raw = zlib.decompress(data[26:])
        n = nx * ny * nz
        g = VoxelGrid(nx, ny, nz, (x0, x1, y0, y1, z0, z1))
        g.density = bytearray(raw[:n])
        g.material = bytearray(raw[n:2 * n])
        g.buildable = bytearray(raw[2 * n:2 * n + nx * nz])
        g.flat_permil = flat
        return g


if __name__ == "__main__":
    import sys
    if len(sys.argv) >= 2:
        g = VoxelGrid.load(sys.argv[1])
        solid = sum(1 for d in g.density if d >= ISO)
        print(f"{sys.argv[1]}: {g.nx}x{g.ny}x{g.nz} bounds={g.bounds}")
        print(f"  solid voxels: {solid}/{len(g.density)}  buildable: {g.flat_permil/10:.1f}%")
    else:
        print("usage: python3 voxel.py <map.vxl>   (inspect a baked map)")
