#!/usr/bin/env python3
"""Bake each world's voxel/density map to a static `.vxl` file.

These are the editable map files: generated once here, then loaded by the engine
(`apps/client/src/voxel.rs`) and editable later by hand via `voxel.py`'s ops or
any tool that speaks the documented VXL1 format. Output: `assets/maps/<key>.vxl`.

Run:  python3 assets/worldgen/bake.py            # all worlds
      python3 assets/worldgen/bake.py moon io     # a subset
"""

import os
import sys

import worlds as cat
import densitygen as dg

OUT = os.path.join(os.path.dirname(__file__), "..", "maps")


def main():
    os.makedirs(OUT, exist_ok=True)
    keys = [a for a in sys.argv[1:] if not a.startswith("-")] or [w["key"] for w in cat.WORLDS]
    total = 0
    for key in keys:
        grid, stats = dg.build(cat.by_key(key))
        path = os.path.join(OUT, key + ".vxl")
        grid.save(path)
        size = os.path.getsize(path)
        total += size
        print(f"  {key:10s} {grid.nx}x{grid.ny}x{grid.nz}  "
              f"buildable {stats['flat']*100:4.1f}%  {size/1024:6.1f} KB")
    print(f"wrote {len(keys)} maps to {os.path.normpath(OUT)} ({total/1024:.0f} KB total)")


if __name__ == "__main__":
    main()
