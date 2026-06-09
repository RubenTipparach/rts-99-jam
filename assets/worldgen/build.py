#!/usr/bin/env python3
"""One-command world-gen build.

Regenerates the texture sets (assets/textures/worlds/) and the preview
screenshots + contact sheet (docs/worldgen/), all from the catalog in
`worlds.py`. Pure stdlib; no GPU and no install step.

Run:  python3 assets/worldgen/build.py            # textures + all previews
      python3 assets/worldgen/build.py --fetch     # also refresh NASA refs (network)

Tip: WORLDGEN_QUICK=1 makes the previews render small and fast for iteration.
"""

import sys

import textures
import render


def main():
    if "--fetch" in sys.argv:
        import fetch_nasa
        fetch_nasa.main()
    made = textures.generate()
    print(f"textures: {len(made)} archetype sets -> assets/textures/worlds/")
    render.main()


if __name__ == "__main__":
    main()
