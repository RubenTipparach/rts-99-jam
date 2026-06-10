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

## Meshing pipeline

The 3D view's marching cubes is **chunked** (32x32-cell columns): a brush stroke
only remeshes the chunks it touched, each into its own GL buffer set. The
meshing itself runs in a **Web Worker**, and inside the worker it prefers the
**Rust/WASM mesher** (`tools/mesher`, checked in as `mesher.wasm` next to this
file); the in-page JS mesher is the fallback when the wasm cannot be fetched
(file://) or instantiated, and a synchronous path remains if workers are
unavailable. The status line says which path ran (`wasm` / `worker` / neither).

After changing `tools/mesher`, rebuild and re-commit the binary:

```sh
cargo build -p mesher --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/mesher.wasm tools/mapeditor/
```

## What you can edit

It edits the map's **3D density field directly** (density is the source of truth),
so it supports real voxel work, not just a heightfield. Layout is **2D / Split /
3D**; the 3D pane is the **marching-cubes surface, textured with the world's tiles**
(triplanar), so it looks like the game.

**3D pane:** **left-drag = current tool**, **right-drag orbits**,
**shift/middle-drag pans**, **wheel zooms**. **Split** shows both panes and the 2D
top-down (with Relief / Material / Buildable / Liquid overlays) updates live.

Tools (brush radius + strength):

- **Voxel + / Voxel -** - add or carve a sphere of solid voxels in 3D: overhangs,
  caves, arches, bumps. The true voxel brush.
- **Raise / Lower / Smooth / Flatten** - shape the heightfield surface (a column
  operation; clicking in 3D applies it at the hit column).
- **Paint mat** - set the material (low / mid / high / accent / hazard); added
  voxels take the selected material too.
- **Buildable** - toggle the per-column buildable mask.
- **Liquid** - fill columns up to the liquid-surface slider, or erase.
- **Eyedrop** - sample height + material.

The 3D mesh rebuilds (marching cubes) when you finish a stroke; on a big map that
is a fraction of a second. **Skin thickness** sets how deep a painted surface
material reaches before the surface tools show the subsurface beneath.

## Export

Export writes the live density / material / buildable / liquid straight to a
`.vxl`, so **everything you edit, including carved caves and overhangs, is kept**.
Files are byte-compatible with the engine loader and the Python tools; a load ->
export round-trip of an unedited map reproduces an equivalent map.

To open or port a map as **text** (e.g. for another editor), convert it with
`python3 assets/worldgen/voxel.py <map.vxl> --json` and back with `--from-json`
(a lossless round-trip). The `.vxl` itself stays binary only for size.
