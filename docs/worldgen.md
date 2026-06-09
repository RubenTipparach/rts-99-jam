# World generation: battlefields across the Solar System

[← Back to ARCHITECTURE.md](ARCHITECTURE.md) · See also [Ch.08 Procedural Map Generation](architecture/08-procedural-generation.md)

The game can be fought over real Solar System bodies. This page documents the
**21 battlefield worlds**, the **texture archetypes** they share, the **map
hazards** on each, and the **marching-cubes voxel terrain** that builds them.
Everything here is presentation-side (floats / assets); none of it touches the
deterministic sim (see [`CLAUDE.md`](../CLAUDE.md), "two worlds, one wall").

- Catalog + generator: [`assets/worldgen/`](../assets/worldgen/) (pure Python stdlib)
- Baked map files: [`assets/maps/<key>.vxl`](../assets/maps/) (static, editable)
- Texture sets: [`assets/textures/worlds/`](../assets/textures/worlds/)
- NASA references: [`worldgen/nasa_references.json`](worldgen/nasa_references.json)

## The worlds at a glance

![All 21 worlds](worldgen/contact_sheet.png)

Each map is a sculpted 3D **density grid** meshed with **marching cubes**, not a
heightmap. That is deliberate: terraced plateaus give large, flat, buildable
tops, the tier boundaries become crisp cliffs, and there are no high-frequency
heightmap spikes. Going fully 3D also allows caves and overhangs, which a
heightmap cannot represent. Green in the previews marks the relatively flat,
**buildable** ground.

## Terrain design: buildable, not spiky

The brief: a guaranteed share of relatively flat, buildable ground; plateaus and
cliffs are good; spikes are not. The recipe (`densitygen.py`):

1. A smooth, **low-frequency** base surface (no fine noise), plus a few wide,
   shallow craters with flat floors (never spikes).
2. **Terracing** into a few big elevation tiers: gentle slopes snap to flat
   plateau tops separated by short cliff risers. The tier count is searched per
   world to hit its buildable target.
3. **Voxelize** to a density field with a thin vertical iso-band so marching
   cubes produces a clean surface; tier jumps become near-vertical cliffs.
4. Carve a few **caves / arches** into the steep (non-buildable) ground for 3D
   interest, leaving build space intact.
5. Mark a per-column **buildable mask** (the flat tops) and store it in the map.

Buildable fractions land between ~58% (rough, cratered worlds like Miranda) and
~90% (smooth resurfaced ice like Europa), so every map has ample base-building
room while staying visually distinct.

## Texture archetypes

Many bodies share a look (barren cratered rock, dirty ice), so they are grouped
into archetypes; a few are unique. A per-world tint, seed and terrain recipe make
each distinct, so Ganymede's cool grooved ice never reads like the Moon's warm
grey rock, and Europa's red-brown lineae never look like Enceladus's blue tiger
stripes.

| archetype | look | worlds |
| --------- | ---- | ------ |
| `regolith_grey` | Airless grey rock (lunar / large-asteroid / Uranian-moon regolith) | moon, vesta, titania, oberon |
| `regolith_dark` | Dark carbonaceous rock, bright salt accents | ceres, umbriel, chiron |
| `dirty_ice` | Cratered brown-grey ice with cool, bright icy highs | callisto, rhea, iapetus, dione |
| `grooved_ice` | Dark ice cut by bright bluish grooves (sulci) | ganymede, ariel, miranda |
| `bright_ice` | Brilliant fresh ice, blue fracture accents | enceladus |
| `europa_ice` | UNIQUE: bright tan ice scored by red-brown lineae + chaos | europa |
| `mars_rust` | UNIQUE: rusty dust and basalt, bright polar ice | mars |
| `io_sulfur` | UNIQUE: sulfur over black volcanism, red pyroclastics | io |
| `titan_haze` | UNIQUE: orange organic haze, dark dunes, methane lakes | titan |
| `triton_ice` | UNIQUE: pinkish nitrogen ice, cantaloupe terrain | triton |
| `pluto_tholin` | UNIQUE: tan tholins beside bright nitrogen plains | pluto |

Each archetype ships four tileable tiles (`base`, `low`, `high`, `accent`) for
texturing the mesh; the previews colour the mesh from the same palettes.

## The full catalog (buildable % and map hazards)

| world | archetype | buildable | map hazards | NASA reference |
| ----- | --------- | --------- | ----------- | -------------- |
| Luna (Moon) | `regolith_grey` | 68% | basalt maria; ray craters | LRO (PIA23237) |
| Ceres | `regolith_dark` | 66% | brine eruptions (faculae); ray craters | Dawn (PIA21078) |
| Vesta | `regolith_grey` | 63% | cliffs/scarps; ray craters | Dawn (PIA15140) |
| Mars | `mars_rust` | 66% | dust storms; polar frost; canyon scarps | Viking / MRO (PIA00565) |
| Callisto | `dirty_ice` | 66% | radiation; ray craters | Galileo (PIA03456) |
| Ganymede | `grooved_ice` | 63% | radiation; ice rifts | Galileo / Juno (PIA05077) |
| Europa | `europa_ice` | 90% | ice rifts (lineae); chaos terrain; radiation | Galileo (PIA00294) |
| Io | `io_sulfur` | 71% | lava lakes; radiation; volcanic plumes | Galileo / Voyager 1 (PIA02509) |
| Titan | `titan_haze` | 80% | methane seas; organic haze; cryo-geysers | Cassini / Huygens (PIA12778) |
| Enceladus | `bright_ice` | 85% | cryo-geysers (tiger stripes); ice rifts | Cassini (PIA03551) |
| Triton | `triton_ice` | 85% | cryo-geysers (N2 plumes); polar frost | Voyager 2 (PIA00056) |
| Rhea | `dirty_ice` | 70% | ray craters; ice cliffs | Cassini (PIA21904) |
| Iapetus | `dirty_ice` | 64% | equatorial ridge; albedo dichotomy | Cassini (PIA21347) |
| Dione | `dirty_ice` | 66% | wispy ice cliffs (chasmata); ray craters | Cassini (PIA21349) |
| Titania | `regolith_grey` | 59% | fault canyons (Messina); ray craters | Voyager 2 (PIA01361) |
| Oberon | `regolith_grey` | 66% | dark crater floors; scarps | Voyager 2 (PIA00034) |
| Umbriel | `regolith_dark` | 65% | bright Wunda ring; radiation | Voyager 2 (PIA00040) |
| Ariel | `grooved_ice` | 62% | graben rift valleys; scarps | Voyager 2 (PIA00037) |
| Miranda | `grooved_ice` | 58% | Verona Rupes (~20 km cliff); coronae rifts | Voyager 2 (PIA18185) |
| Pluto | `pluto_tholin` | 69% | nitrogen glaciers (Sputnik Planitia); frost; cryovolcano | New Horizons (PIA09234) |
| Chiron | `regolith_dark` | 64% | comet jets (outgassing); ray craters | Deep Space 1 analog (PIA03865) |

### Hazard glossary

Hazards are declared per world in `worlds.py` (and meant to drive gameplay terrain
effects): **lava lakes / volcanic plumes** (Io), **methane seas** (Titan),
**cryo-geysers** (Enceladus, Triton, Pluto), **ice rifts / chaos / grooves**
(Europa, Ganymede, Ariel, Miranda, Enceladus), **dust storms** (Mars, Titan),
**brine eruptions** (Ceres), **cliffs / scarps** (Miranda, Vesta, Dione, the
Uranian moons), **albedo dichotomy / equatorial ridge** (Iapetus), **nitrogen
glaciers** (Pluto), **radiation** (the Jovian moons), and **comet jets** (Chiron).

## The map file format (`.vxl`)

A baked map is a tiny, documented container (`voxel.py`). The canonical, editable
artifact is the raw density grid:

```
magic   "VXL1"                     4 bytes
nx ny nz                           3 x uint16  (grid dims; y is up)
x0 x1 y0 y1 z0 z1                  6 x int16   (world-space bounds)
flat_permil  reserved             2 x uint16   (buildable fraction * 1000)
payload = zlib( density[nx*ny*nz] + material[nx*ny*nz] + buildable[nx*nz] )
          density: uint8 (>=128 solid),  material: uint8,  buildable: uint8 mask
```

The 21 maps total under 400 KB. Edit later with the ops on `VoxelGrid`
(`fill_box`, `carve_sphere`, ...), `python3 assets/worldgen/voxel.py <map.vxl>`
to inspect, or any tool that speaks VXL1.

## Engine integration

The deterministic sim is flat 2D fixed-point, so the chosen design is a
**marching-cubes mesh + cliffs for the look, with buildability derived from the
baked `buildable` mask on a 2.5D plane**. Implemented in
[`apps/client/src/voxel.rs`](../apps/client/src/voxel.rs):

- Parses the embedded `.vxl` maps (zlib via `miniz_oxide`, wasm-friendly), runs
  marching cubes to a coloured mesh, and exposes `surface_height(x, z)`.
- When a map is selected (`voxel::ACTIVE`, default none), `gfx.rs` draws that
  mesh (via the vertex-coloured unit pipeline) instead of the heightmap terrain
  and ocean, and `terrain.rs` sits units on `surface_height`. Tests parse and
  mesh all 21 maps; CI (fmt / clippy / test / wasm) is green.
- The default keeps the existing Earthlike map, so the live build is unchanged
  until a battlefield is selected.

Remaining, determinism-sensitive follow-up: feeding the buildable mask and a 3D
surface into the deterministic sim for in-sim pathing/building. Anything the sim
consumes must be fixed-point and re-pins the golden hash, so it is staged
carefully rather than rushed. (The parameterized heightmap procgen in
`apps/client/src/worlds.rs` remains as a lightweight fallback.)

## Regenerating

```sh
python3 assets/worldgen/build.py            # textures + baked maps + previews
python3 assets/worldgen/build.py --fetch    # also refresh the NASA references (network)
python3 assets/worldgen/render3d.py moon io # fast subset preview
python3 assets/worldgen/bake.py             # just the .vxl maps
```

See [`assets/worldgen/README.md`](../assets/worldgen/README.md) for the module
breakdown.

## Imagery credit

Palettes are tuned to match real spacecraft imagery: NASA / JPL-Caltech and
partner missions (LROC, Dawn, Galileo, Cassini-Huygens, Voyager 2, Juno, New
Horizons, Deep Space 1, Hubble). Most NASA imagery is public domain; see the NASA
[media usage guidelines](https://www.nasa.gov/multimedia/guidelines/). The
generated tiles, maps and previews here are original procedural art, not NASA
imagery.
