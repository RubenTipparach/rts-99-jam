# worldgen

Dependency-free (Python stdlib only) tooling that generates the **battlefield
worlds**: the Solar System bodies the game can be fought over (the Moon, Mars,
the major moons of Jupiter / Saturn / Uranus / Neptune, Pluto, and the centaur
Chiron). It produces:

- **Texture sets** per archetype in `assets/textures/worlds/<archetype>/`
  (drop-in for the client's four-sampler terrain shader), and
- **Preview screenshots** + a contact sheet in `docs/worldgen/`, rendered with a
  small built-in oblique heightfield renderer so the procgen + textures can be
  eyeballed without a GPU.

The engine-side mirror of the terrain recipes lives in
`apps/client/src/worlds.rs`. See `docs/worldgen.md` for the full catalog,
archetypes, hazards and the NASA references the palettes are tuned from.

## Run

```sh
python3 assets/worldgen/build.py            # textures + all previews + contact sheet
python3 assets/worldgen/build.py --fetch    # also refresh docs/worldgen/nasa_references.json (network)

# iterate fast (small, low-res previews):
WORLDGEN_QUICK=1 python3 assets/worldgen/render.py europa io titan

# regenerate just the texture sets:
python3 assets/worldgen/textures.py
```

## Files

| file            | what it does                                                       |
| --------------- | ----------------------------------------------------------------- |
| `worlds.py`     | the catalog: archetypes (palettes) + per-world terrain & hazards  |
| `textures.py`   | paints the tileable texture sets                                  |
| `render.py`     | procedural terrain + masks, then the oblique hill-shaded previews |
| `fetch_nasa.py` | pulls a representative NASA reference image (metadata) per world  |
| `common.py`     | PNG io, value noise, RNG, colour helpers, a 5x7 label font        |
| `build.py`      | one-command runner                                                |

Everything is deterministic: re-running produces byte-identical output (no
salted hashes, no wall-clock). These are presentation assets only; nothing here
touches the deterministic simulation (see `CLAUDE.md`).
