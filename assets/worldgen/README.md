# worldgen

Dependency-free (Python stdlib only) tooling that generates the **battlefield
worlds**: the Solar System bodies the game can be fought over (the Moon, Mars,
the major moons of Jupiter / Saturn / Uranus / Neptune, Pluto, and the centaur
Chiron). It produces:

- **Baked voxel maps** in `assets/maps/<key>.vxl` (static 3D density grids; the
  editable map files),
- **Texture sets** per archetype in `assets/textures/worlds/<archetype>/`, and
- **Preview screenshots** + a contact sheet in `docs/worldgen/`, rendered with a
  small built-in marching-cubes z-buffer renderer so the terrain + buildability
  can be eyeballed without a GPU.

Terrain is a sculpted 3D density field meshed with **marching cubes** (not a
heightmap), terraced into flat buildable plateaus and crisp cliffs. See
`docs/worldgen.md` for the catalog, archetypes, hazards, the `.vxl` format and
the NASA references the palettes are tuned from.

## Run

```sh
python3 assets/worldgen/build.py            # textures + baked maps + previews + contact sheet
python3 assets/worldgen/build.py --fetch    # also refresh docs/worldgen/nasa_references.json (network)

python3 assets/worldgen/render3d.py moon io europa   # fast subset preview
python3 assets/worldgen/bake.py                      # just (re)bake the .vxl maps
python3 assets/worldgen/densitygen.py                # print per-world buildable %
python3 assets/worldgen/voxel.py assets/maps/io.vxl  # inspect a baked map
```

## Files

| file               | what it does                                                      |
| ------------------ | ---------------------------------------------------------------- |
| `worlds.py`        | the catalog: archetypes (palettes) + per-world terrain & hazards |
| `densitygen.py`    | builds the per-world voxel density field (terraces, flat target, caves) |
| `marching_cubes.py`| polygonizes the density grid (tetrahedral marching cubes)        |
| `voxel.py`         | the `.vxl` map format (read/write) + editing ops + CLI inspect   |
| `bake.py`          | bakes every world to `assets/maps/<key>.vxl`                     |
| `render3d.py`      | z-buffered marching-cubes previewer (the screenshots)           |
| `textures.py`      | paints the tileable texture sets                                 |
| `fetch_nasa.py`    | pulls a representative NASA reference image (metadata) per world |
| `common.py`        | PNG io, value noise, RNG, colour helpers, a 5x7 label font       |
| `build.py`         | one-command runner                                               |

Everything is deterministic: re-running produces byte-identical output (no salted
hashes, no wall-clock). These are presentation assets only; nothing here touches
the deterministic simulation (see `CLAUDE.md`).
