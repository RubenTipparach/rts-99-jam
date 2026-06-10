# Faction asset kit - implementation handoff

This is the handoff package for getting the approved faction concept art into
the game. The concepts (buildings, workers, resource nodes) were signed off and
the **assets are already committed**; this doc records the exact materials,
geometry, and animation specs, plus step-by-step instructions, so a future
session can implement the rest without re-deriving anything.

Read together with:

- [`building-design-language.md`](building-design-language.md) - the design
  language (silhouette rules, faction identity, footprint tiers, state cues).
- [`factions.md`](factions.md) - lore + roster roles.

---

## 1. Asset inventory (what is already in the repo)

| File | What it is |
|---|---|
| `assets/concepts/buildings.png` | Approved sheet: 5 Astromancer + 5 Hollowmen buildings |
| `assets/concepts/units_resources.png` | Approved sheet: worker anim poses + ore/carbon nodes |
| `assets/render_buildings.py` | **Geometry source of truth** for the 10 buildings (part lists, dimensions, colors) |
| `assets/render_units.py` | **Geometry source of truth** for worker poses + resource nodes + gas smoke |
| `assets/concept_kit.py` | Shared concept renderer (camera, primitives, shading) |

The Python scripts are pure stdlib and deterministic: running them reproduces
the committed PNGs byte for byte (`python3 assets/render_buildings.py`,
`python3 assets/render_units.py`). They render with the game's exact iso camera
(yaw 45 deg, pitch 0.95 rad), so proportions on the sheets are what you will
see in-engine.

### Implementation status

| Asset | Concept fn (Python) | In-game | Where |
|---|---|---|---|
| Astro Spire (HQ) | `astro_spire` | DONE (adapted as the Astro production building) | `barracks_mesh_astro` in `apps/client/src/gfx.rs` |
| Astro Reliquary | `astro_reliquary` | pending | - |
| Astro Sanctum | `astro_sanctum` | pending | - |
| Astro Citadel | `astro_citadel` | pending | - |
| Astro Sky-Cradle | `astro_skycradle` | pending | - |
| Hollowmen Command HQ | `holl_command` | pending | - |
| Hollowmen Refinery | `holl_refinery` | pending | - |
| Hollowmen Reactor | `holl_reactor` | pending | - |
| Hollowmen Factory | `holl_factory` | DONE (adapted as the Hollowmen production building) | `barracks_mesh_hollow` in `gfx.rs` |
| Hollowmen Null Pylon | `holl_nullpylon` | pending | - |
| Acolyte (idle pose) | `acolyte("IDLE")` | DONE | `acolyte_mesh` in `gfx.rs` |
| Acolyte (grow / gather poses) | `acolyte("GROW"/"GATHER")` | pending (single mesh + bob today) | - |
| Engineer (neutral pose) | `engineer(...)` | DONE | `engineer_mesh` in `gfx.rs` |
| Engineer (walk / build / gather poses) | `engineer("WALK"/"BUILD"/"GATHER")` | pending (single mesh + bob today) | - |
| Ore node (full + depleting) | `ore_node` | DONE (depletion = uniform shrink) | `ore_node_mesh` in `gfx.rs` |
| Carbon geyser (mound + crater) | `carbon_node` | DONE | `carbon_node_mesh` in `gfx.rs` |
| Carbon green smoke | `draw_smoke` | pending (needs a transparent particle pass) | - |
| Turret, Heavy | (no concept; placeholder designs) | DONE | `turret_mesh`, `heavy_mesh` in `gfx.rs` |

---

## 2. Material palettes (canonical)

Canonical values are the constants at the top of `assets/render_buildings.py`
and `assets/render_units.py` (the unit sheet tweaks a few by 1-3 points; use the
values below). "Emissive" marks faces that read as glowing on the sheets - see
section 3 for how to fake that in-engine.

### Astromancers - grown ivory, violet/teal energy, gold trim

| Name | sRGB | Hex | Use |
|---|---|---|---|
| `A_SHELL` | 238, 233, 219 | `#EEE9DB` | Primary grown shell |
| `A_SHELL2` | 216, 211, 194 | `#D8D3C2` | Secondary shell (upper tapers, mantles) |
| `A_SHELL3` | 184, 179, 162 | `#B8B3A2` | Shadowed shell (roots, undersides, plinths) |
| `A_VIO` | 168, 92, 222 | `#A85CDE` | Violet crystal body |
| `A_COREV` | 210, 150, 255 | `#D296FF` | Violet core glow (emissive) |
| `A_TEAL` | 96, 224, 210 | `#60E0D2` | Teal energy seam / band (emissive) |
| `A_CORET` | 160, 255, 246 | `#A0FFF6` | Teal core glow, eyes, motes (emissive) |
| `A_GOLD` | 234, 204, 126 | `#EACC7E` | Gold ceremonial trim, spire caps |

### Hollowmen - gunmetal steel, hazard amber, cyan readouts

| Name | sRGB | Hex | Use |
|---|---|---|---|
| `H_STEEL` | 124, 132, 142 | `#7C848E` | Primary hull steel |
| `H_STEEL2` | 96, 104, 114 | `#606872` | Darker steel (roofs, stacks, masts) |
| `H_PLATE` | 158, 166, 174 | `#9EA6AE` | Bright plate armor highlights |
| `H_DARK` | 66, 72, 80 | `#424850` | Foundations, doors, crowns |
| `H_GUN` | 48, 52, 58 | `#30343A` | Weapons, barrels, boots |
| `H_HAZ` | 214, 158, 54 | `#D69E36` | Hazard stripe / skirt |
| `H_AMBER` | 250, 196, 96 | `#FAC460` | Amber vents / running lights (emissive) |
| `H_CYAN` | 110, 196, 224 | `#6EC4E0` | Cyan windows / readouts (emissive) |
| `H_RUST` | 150, 96, 60 | `#96603C` | Rust accent (sparingly) |
| Null-field glow | 150, 110, 220 | `#966EDC` | Null Pylon emitter rings (emissive) |
| Null-field glow 2 | 170, 130, 240 | `#AA82F0` | Null Pylon emitter head (emissive) |

### Resources (never team tinted)

| Name | sRGB | Hex | Use |
|---|---|---|---|
| `ORE_BODY` | 120, 214, 236 | `#78D6EC` | Crystal shard body |
| `ORE_BODY` lt | 150, 226, 244 | `#96E2F4` | Lighter shard variant |
| `ORE_BODY` dk | 96, 196, 224 | `#60C4E0` | Deeper shard variant |
| `ORE_CORE` | 210, 248, 255 | `#D2F8FF` | Near-white tips + inner glow (emissive) |
| `ORE_DEEP` | 70, 150, 196 | `#4696C4` | Carried-ore canister |
| `ROCK` | 72, 82, 96 | `#485260` | Ore rock base |
| `ROCK_DK` | 44, 52, 64 | `#2C3440` | Ore rock base, lower |
| `GAS_GLOW` | 130, 244, 156 | `#82F49C` | Geyser crater glow (emissive) |
| `GAS_CORE` | 200, 255, 210 | `#C8FFD2` | Crater hot center (emissive) |
| `GAS_SMOKE` | 120, 228, 140 | `#78E48C` | Rising smoke puff tint |
| `VENT` | 54, 64, 60 | `#36403C` | Geyser mound rock |
| `VENT_DK` | 34, 42, 40 | `#222A28` | Geyser mound, lower |
| Glint | 255, 255, 255 | `#FFFFFF` | Tiny sparkle motes on crystal tips |

**In-engine conversion.** Mesh colors in `gfx.rs` are `[f32; 3]`, and the
convention is simply `sRGB / 255` (e.g. `H_STEEL` -> `[0.48, 0.51, 0.55]`,
`ORE_BODY` -> `[0.47, 0.84, 0.92]`). The one deliberate exception: very bright
ivory shells were dropped ~8% (`A_SHELL` -> `[0.86, 0.84, 0.76]`) so the lit
top faces do not blow out. Start with `/255`, then nudge by eye against the
existing meshes.

---

## 3. Authoring conventions (engine rules)

These match every existing mesh in `apps/client/src/gfx.rs`; follow them.

- **Units and axes.** 1 mesh unit = 1 world unit, y-up, ground at `y = 0`.
  Units and buildings **face -z**.
- **Scale.** Concept dimensions are close to world scale but were enlarged when
  ported: buildings ~x1.15-1.25 (concept half-width 4.2 became 5.2 in-game),
  ore/carbon nodes x2 (they are map features, ~6-8 wide), workers ~x1.25
  (concept ~2.2 tall, in-game ~2.7-2.95; infantry are ~2.4). Footprints must
  stay honest: production-tier buildings get selection-ring radius 8 and sim
  obstacle radius 6; small/defense tier ring 4, obstacle radius 2.5.
- **Hover (Astromancers).** Buildings hang their grown root in the air with a
  visible gap baked into the mesh (root bottom ~y 0.5, body from ~y 1.6); the
  instance still sits at terrain height. Workers are authored just above y 0
  and lifted by the per-frame instance offset instead (so the bob can dip).
- **Team tint.** `UnitVertex.color.a` is the tint weight: 0 = material color,
  1 = full team color. Paint team-tinted parts mid-gray `[0.5, 0.5, 0.5]` with
  weight 1.0. One reserved tint zone per design language: Astro = the energy
  core/crystals, Hollowmen = the window band / visor / housing. Resources never
  tint.
- **"Emissive" faces.** The unit pipeline has no emissive channel. Glow is
  faked exactly like the concepts fake shine: paint the part its bright glow
  color, add a brighter inset core (a slightly smaller prism inside, see
  `ore_node_mesh`), and near-white tips. (A real emissive bit could ride a
  spare vertex attribute later; not required for this kit.)
- **Primitive kit.** Compose from the helpers in `gfx.rs`; the concept scripts
  use the same vocabulary, so porting is mechanical:

  | Python (`render_buildings.py`) | Rust (`gfx.rs`) |
  |---|---|
  | `box(m, p0, p1, c)` | `push_box(&mut m, min, max, rgb, team)` |
  | `prism(m, cx, cz, r, y0, y1, c, n, rot, top)` | `push_prism(&mut m, cx, cz, r, y0, y1, rgb, team, n, rot, top)` |
  | `frustum(m, cx, cz, r0, r1, y0, y1, c, n, rot, top)` | `push_frustum(...)` (same arg order) |
  | `pyramid(m, cx, cz, r, y0, y1, c, n, rot)` | `push_pyramid(...)` |
  | `gable(m, x0, z0, x1, z1, y0, yk, c)` | `push_roof(&mut m, cx, cz, hx, hz, base_y, peak_h, rgb)` (note: center + half-extents, peak height relative) |
  | `crystal(m, cx, cz, r, y0, ymid, ytip, body, tip)` | no helper yet; expand to `push_prism(.., r, y0, ymid, body, .., n=6, top=false)` + `push_pyramid(.., r, ymid, ytip, tip, .., n=6)` (worth adding a `push_crystal` helper, the Astro set uses it constantly) |

  Backface winding never matters: `push_tri` orients normals outward from the
  primitive center automatically.
- **Floats are fine here.** All of this is presentation (`apps/client`); none
  of it touches the sim, the hash, or determinism. Pure art changes never
  require re-pinning `GOLDEN`.

### Geometry source of truth

Do **not** re-model from the PNGs. Transcribe the part lists directly from the
Python functions: every `box`/`prism`/`frustum`/`pyramid`/`crystal` call is one
`push_*` call with the same numbers (then apply the scale factor and faction
hover rules above). Worked example: compare `astro_spire()` in
`render_buildings.py` with `barracks_mesh_astro()` in `gfx.rs` - same parts,
~x1.15 scale, the teal band and orbiting shards became the team-tint zones.

---

## 4. Animation spec

All animation is client-side, computed per frame in `Game::render_data`
(`apps/client/src/game.rs`) by writing the per-instance `offset` / `scale`.
Desync between phases is by unit: `phase = s.index as f32 * 1.3`.

### Already implemented (keep these behaviors)

| What | Spec (as shipped) |
|---|---|
| Acolyte hover + bob | `y = ground + hover + sin(time * 2.2 + phase) * 0.18`, hover = 1.1 idle, **0.5 while mining** (dips to gather) |
| Engineer walk bob | when moving and not mining: `y += abs(sin(time * 9.0 + phase)) * 0.12` |
| Worker "working" bob | while `s.mining`: `work = abs(sin(time * 14.0 + phase))`; Engineer adds `work * 0.10` (drill judder) |
| Infantry march | when moving: `y += sin(time * 9.0 + phase) * 0.12`; per-unit size jitter `0.92..1.08` |
| Heavy march | when moving: rate 6.0, amplitude 0.08; uniform scale 1.55 |
| Construction rise | building `scale.y = construct_frac.clamp(0.08, 1.0)` (sim drives `construct_frac` 0 -> 1 over `CONSTRUCT_TICKS`) |
| Node depletion | node uniform scale `= 0.55 + 0.45 * resource_frac` (full -> stub) |

`Snap` (in `game.rs`) already carries the state flags these need: `moving`,
`mining`, `resource_frac`, `construct_frac`.

### Pending - worker action poses (from the approved sheet)

The concept sheet defines **pose states**, and the cleanest engine fit is
**pose-mesh swapping**: author 2-3 mesh variants per worker and pick the
bucket per frame in `render_data` (each variant is just another instanced mesh
group, exactly like `acolytes` vs `engineers` today).

| State | Acolyte (from `acolyte(pose)`) | Engineer (from `engineer(pose)`) |
|---|---|---|
| Idle / move | as shipped (orb at chest) | as shipped (wrench carried) |
| Build | orb raised above hood; vertical teal beam (`A_TEAL`, emissive) from orb upward | right arm raised, welder tool forward, 2-3 amber spark motes near the tool tip |
| Gather | orb lowered to ~waist height in front; dip-hover 0.5 | drill arm down, drill bit pointing into the ground; ore canister on the back glows `ORE_CORE` |

State selection: `mining` exists on `Snap`; "building" needs one more bool
derived from the unit's order in the sim snapshot (Order::Build / arrived).
That is a read-only presentation field on `Snap`: **the hash does not cover
snapshots**, so it does not re-pin `GOLDEN` - but run the determinism test
anyway after touching `crates/sim`.

Swap cadence for two-frame walk cycles (if you add the Engineer's posed legs:
one leg forward at z `0.5..0.9`, one back at z `-0.7..-0.3`): toggle meshes at
~3 Hz scaled by speed, i.e. frame = `(time * 3.0 + phase) as i32 % 2`.

### Pending - ambient building/site motion

| Effect | Spec |
|---|---|
| Astro building bob | whole-instance `y += sin(time * 0.8 + phase) * 0.10` (slow, subtle; buildings only, never Hollowmen) |
| Orbiting shards (Spire/Citadel) | small crystal mesh instanced separately; CPU-orbit the instance offset: radius 4.5-5, angular speed 0.9 rad/s, y bob 0.3 |
| Null Pylon ring pulse | emitter rings authored at y 3/4/5 (concept scale); pulse by instancing them as a separate mesh with `scale = 1.0 + 0.05 * sin(time * 6.0)` |
| Smokestack puffs (Hollowmen) | same particle pass as geyser smoke, smaller: spawn at stack tip, radius 0.25 -> 0.9, life ~1.6 s, gray `[0.45, 0.47, 0.5]`, alpha 0.35 -> 0 |

### Pending - carbon geyser green smoke (the one real new system)

The concept's `draw_smoke` is a 2D post-effect; in-engine it needs a
**transparent billboard particle pass** (the opaque unit pipeline cannot do
fades). Spec, converted to world units:

- Emitter: crater center, spawn height = mound top (~y 3.2 at full scale).
- 6-8 live puffs per geyser; spawn one every ~0.4 s, lifetime ~3 s.
- Per puff over its life `t` (0 -> 1): rise `y += 9.0 * t`, lateral drift
  `x += 0.8 * sin(seed + t * 3)`, radius `0.6 -> 2.4`, alpha `0.55 -> 0`.
- Color `GAS_SMOKE` `[0.47, 0.89, 0.55]`, slightly whitened near spawn.
- Render: camera-facing quads, alpha blend, **depth-test on / depth-write
  off**, drawn after the water pass. Copy the pattern of the existing ring
  pipeline (`ring_pipeline` in `gfx.rs`) for a small dedicated pipeline; the
  water shader shows how the existing passes do alpha blending.
- Scale particle count/size by `resource_frac` so depleted geysers wheeze.

The same pass also covers gather motes (Acolyte: 3 teal `A_CORET` motes
drifting from node to chest, 0.07 radius; Engineer: amber weld sparks) and
smokestack puffs. Build it once, feed it from `render_data`.

---

## 5. How to implement one new building/unit mesh (step by step)

Adding a mesh is a fixed 6-file-touch recipe; `Heavy`/`Turret` (commit
`326b496`) is a complete worked example to diff against.

1. **`apps/client/src/gfx.rs`** - write `fn <name>_mesh() -> Vec<UnitVertex>`
   from the Python part list (section 3). Author at world scale, ground y = 0,
   facing -z, team-tint zone gray + weight 1.
2. **`gfx.rs`, `Gfx::new`** - build the vertex buffer next to the others
   (`let x = x_mesh(); let x_buf = mkbuf("x", ...)`), store `x_buf` and
   `x_len` on `Gfx` like `turret_buf` / `turret_len`.
3. **`gfx.rs`, `Gfx::render`** - add a `&[InstanceRaw]` parameter, extend the
   `groups` array, the `counts` destructure, the `slices` array, and the
   `meshes` draw table (order must match everywhere; instances are packed
   consecutively in one buffer).
4. **`apps/client/src/game.rs`** - add a bucket `Vec<InstanceRaw>` in
   `render_data`, push instances for the right `Kind` (+ faction split via
   `self.faction_of(s.owner)` if the kinds share a sim Kind), apply the
   animation offsets from section 4, extend the `RenderData` tuple type alias.
5. **`apps/client/src/main.rs`** - pass the new bucket through `render_scene`.
6. **Tests** - add the mesh to `unit_meshes_are_well_formed` in `gfx.rs`.
   Then: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D
   warnings`, `cargo test --workspace`, and a wasm check
   (`cd apps/client && trunk build`).

If the buckets multiply (they will: ~10 buildings x poses), consider
refactoring `render` to take `&[&[InstanceRaw]]` plus a parallel mesh array
instead of one parameter each - mechanical, and the packing logic already
loops.

### If the asset needs a new gameplay kind (sim work)

Pure reskins of existing kinds need nothing below. A genuinely new
building/unit type does, in this order:

1. `crates/protocol` - add the `BuildingKind`/`UnitKind` variant.
2. `crates/sim` - add the `Kind` variant; stats, `unit_cost`/`building_cost`,
   `obstacle_radius`, footprint; include it in `state_hash` kind mapping; keep
   every iteration ordered. **No floats / HashMap / RNG** (CI greps for them,
   including comments).
3. Re-pin the golden hash: `cargo run -p testkit --bin demo_hash`, paste into
   `GOLDEN` in `crates/testkit/tests/determinism.rs`.
4. Client wiring as above, plus HUD command card labels (`hud.rs`) and
   build-placement (`build_mode` in `main.rs`) if the player can place it.

---

## 6. Definition of done for this kit

- [ ] 8 remaining buildings ported as meshes (section 1 table), correct
      footprint tier, team-tint zone, hover gap on all Astro structures.
- [ ] Worker pose meshes (build + gather, both factions) swapped by state.
- [ ] Transparent particle pass: geyser smoke, gather motes, weld sparks.
- [ ] Ambient motion: Astro bob, pylon pulse, orbiting shards.
- [ ] All meshes in `unit_meshes_are_well_formed`; fmt/clippy/tests/wasm green.
- [ ] No DOM UI added (HUD canvas only), no doc files outside `docs/`,
      no em-dashes anywhere.
