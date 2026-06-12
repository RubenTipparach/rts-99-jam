# Model assets

Static Wavefront OBJ/MTL models for every unit, building, and resource
node. **These files are the source of truth**: edit them in Blender (or
any OBJ-capable tool) and commit the result. `tools/modelgen`
(`cargo run -p modelgen`) only scaffolds brand-new models and OVERWRITES
everything on re-run, so never use it to "refresh" a hand-edited file;
pass model names (`cargo run -p modelgen -- hound javelin`) to scaffold
only those and leave every other file untouched.

Conventions the engine relies on (loader: `apps/client/src/model.rs`):

- Coordinates are world units, +y up, ground plane at y = 0. Keep each
  model's overall footprint and height: selection rings, healthbars, and
  fx anchors are positioned from them, not from the mesh.
- Facing: the turret and the Astromancer ward fire toward -z and the
  infantry faces -z; the workers (acolyte, engineer) and the heavy face
  +z (visor side); building doors face +z. The acolyte is authored
  resting just above y = 0 and is lifted into its hover by the engine.
- Walk cycles are static keyframe assets: `<unit>-walk-0.obj` through
  `-7.obj` next to the idle base model (infantry, engineer, heavy), all
  sharing the base model's `.mtl`. Each frame is a whole-mesh snapshot you
  can edit in Blender; at runtime instances snap to the nearest frame (no
  interpolation, deliberately chunky). `cargo run -p modelgen` scaffolds a
  cycle from the idle pose by swinging geometry below y of about 1.3 by
  the sign of x, so keep each leg on its own side of x = 0 if you want a
  re-scaffold to stay coherent.
- The MTL `Kd` is the material color, freely editable. The material NAME
  carries engine flags that Blender round-trips untouched:
  - `_t1` - team-tint channel: the faction color replaces the material
    color (banners, cores, window bands). Anything else is `_t0`.
  - `_d0` - flat shaded, no surface-detail texture (crystals, gas pools).
  - `_d1` - organic rock/dirt grain (the detail map's green channel).
  - `_d2` - plate/masonry seams (red channel); the default.
- Normals must point outward; the unit pipeline does not cull back faces.
  Faces may be tris, quads, or n-gons (fan triangulated on load); `vn` is
  used when present, otherwise a flat normal comes from the face winding.
- Export from Blender with normals and materials enabled; keep the `.mtl`
  next to its `.obj`.
- `ore-crystal` and `carbon-pool` render through the blended "glassy"
  shader on top of their `ore-node` / `carbon-node` bases and stay flat
  shaded (`_d0`).
