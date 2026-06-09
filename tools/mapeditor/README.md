# Voxel map editor (dev tool)

A single-file, dependency-free browser editor for the baked `.vxl` battlefields in
[`assets/maps`](../../assets/maps). Load a preset (or any `.vxl`), tinker with the
terrain, and export a `.vxl` you can drop back into `assets/maps` and commit. It
speaks the same VXL1 format as `assets/worldgen/voxel.py` and
`apps/client/src/voxel.rs`, and uses the browser's native zlib (Compression
Streams, the `deflate` = RFC 1950 variant), so there is nothing to install.

This is **not** game UI (the in-game HUD is all Rust/WASM); it is an offline
authoring tool, the JS sibling of the Python pipeline in `assets/worldgen`.

## Run

Serve the repo over http so the presets can be fetched, then open the editor:

```sh
python3 -m http.server          # from the repo root
# open http://localhost:8000/tools/mapeditor/
```

Or just open `index.html` directly and use **Load .vxl file** to pick a map by
hand (the preset dropdown needs http to fetch).

Needs a current browser (Chrome / Edge / Firefox / Safari) for `CompressionStream`.

## What you can edit

A top-down view with **Relief / Material / Buildable / Liquid** overlays, and a
**2D / Split / 3D** layout. The 3D pane is an interactive WebGL view of the
surface (height-exaggeration slider): **left-drag paints** with the current tool,
**right-drag orbits**, **shift/middle-drag pans**, **wheel zooms**. **Split**
shows 2D and 3D together and updates the 3D live as you paint in 2D. The game
itself renders the real shaded terrain (lobby -> pick world -> Start).

Tools (brush radius + strength):

- **Raise / Lower / Smooth / Flatten** the surface height.
- **Paint mat** - set the surface material (low / mid / high / accent / hazard).
- **Buildable** - toggle the per-column buildable mask.
- **Liquid** - fill columns up to the liquid-surface slider, or erase.
- **Eyedrop** - sample height + material under the cursor.

**Skin thickness** (export) sets how deep the painted surface material goes before
the column shows its subsurface, matching the skin/subsurface model in
`densitygen.py`.

## How export works (and its limit)

This is a heightfield + material-strata editor. On export, every column you
**edited** is rebuilt as a single-surface column (density from the height ramp,
material as skin-over-subsurface). Columns you did **not** touch are written back
**verbatim** from the loaded map, so caves, overhangs and other true-3D features
elsewhere are preserved exactly.

The trade-off: carved caves/overhangs *inside a column you paint* are flattened.
Author genuinely volumetric features (caves, arches, the per-world landform
recipes) in the Python pipeline (`assets/worldgen/densitygen.py`); use this editor
for surface tinkering and quick fixes.

Exported files are byte-compatible with the engine loader and the Python tools, so
a load -> export round-trip of an unedited map reproduces an equivalent map.

To open or port a map as **text** (e.g. for another editor), convert it with
`python3 assets/worldgen/voxel.py <map.vxl> --json` and back with `--from-json`
(a lossless round-trip). The `.vxl` itself stays binary only for size.
