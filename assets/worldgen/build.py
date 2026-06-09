#!/usr/bin/env python3
"""One-command world-gen build.

Regenerates everything from the catalog in `worlds.py`:
  1. texture sets        -> assets/textures/worlds/<archetype>/
  2. baked voxel maps    -> assets/maps/<key>.vxl   (static, editable)
  3. preview screenshots -> docs/worldgen/previews/ + contact_sheet.png
     (marching-cubes renders of the actual voxel maps; no GPU needed)

Run:  python3 assets/worldgen/build.py            # everything
      python3 assets/worldgen/build.py --fetch     # also refresh NASA refs (network)

Tip: render a subset fast with `python3 assets/worldgen/render3d.py moon io`.
"""

import sys

import textures
import bake
import render3d


def main():
    if "--fetch" in sys.argv:
        import fetch_nasa
        fetch_nasa.main()
    made = textures.generate()
    print(f"textures: {len(made)} archetype sets -> assets/textures/worlds/")
    bake.main()
    render3d.main()


if __name__ == "__main__":
    main()
