# Building design language - Astromancers vs Hollowmen

Design companion to [`factions.md`](factions.md). This doc sets the **visual
language** for structures (and, by extension, units) for the two launch factions,
and ships with a rendered concept sheet so the silhouettes can be reviewed before
any in-engine modelling.

Concept render: `assets/concepts/buildings.png` (regenerate with
`python3 assets/render_buildings.py`). It is **rough massing only**: approximate
shapes, proportions, and palettes in the game's iso camera, not final art.

The **in-engine meshes** (the real `gfx.rs` geometry, with the game camera and
the unit shader's lighting) are previewed in `docs/buildings/previews/*.png`;
regenerate them with
`cargo test -p client render_building_previews -- --ignored` (writes to
`apps/client/target/previews/`, then copy the keepers here).

---

## 1. What we took from Blizzard's RTS design

A short, practical read of StarCraft (1 and 2) and Warcraft III building/unit
design, focused on what we can actually reproduce in a low-poly, vertex-colored,
flat-shaded renderer.

**Silhouette first (readability at a glance).** Blizzard's rule: you should name
a unit or building from its black silhouette alone, at minimap-thumbnail size.
Each structure gets one dominant shape and one signature appendage (a Barracks'
shutter door, a Pylon's floating crystal, a Photon Cannon's eye). We design the
outline before any surface detail.

**Faction identity is carried by form + material + palette, repeated
relentlessly.**
- **Terran:** boxy, bolted, prefab. Concrete-and-steel, exposed girders, blast
  doors, smokestacks, hazard stripes, neon signage. Asymmetric, "assembled in a
  yard." Every building is armed or can be. Grounded and heavy; some lift off,
  but they sit on the dirt.
- **Protoss:** smooth, curved, crystalline, gold-and-azure. Grown/warped-in, not
  built; everything floats or is held aloft by a psionic field. Symmetric,
  ceremonial, jewel-like. Glowing energy seams and a clear power core.
- **Zerg:** organic, asymmetric, wet. (Not a launch faction, noted for contrast:
  it shows how far "material + silhouette" alone pushes identity.)

**A kit of shared parts (kitbashing).** Within a faction, buildings reuse a small
library of modules (Terran: the same door, vent, antenna, plated wall, tank;
Protoss: the same crystal, ring, plinth, gold trim). This reads as a coherent
"tech," makes new buildings cheap to author, and is exactly how we build meshes
here (compose from `push_box` + a few primitives).

**Color = team + state, never decoration.** A reserved team-tint channel (the
banner/trim) is the only place faction color lives, so red vs blue is unmissable.
Energy/glow colors are constant per faction (Terran amber/cyan readouts, Protoss
azure/gold psi-glow) and double as the "powered / working" tell.

**Footprint honesty + grounding.** A building's base matches its collision
footprint (no overhang you can't path around), and it sits in the world with a
contact shadow and a bit of ground decal (Terran tarmac/scorch, Protoss light
ring), so nothing looks pasted on.

**State is legible.** Construction (scaffold/warp-in), powered vs unpowered,
damaged (smoke/cracks), and selected (ground ring) are all readable without text.

---

## 2. Our shared language (engine-aware)

Everything below is chosen to be expressible by the existing renderer:
flat-shaded, vertex-colored boxes and a few primitives (prism, frustum, pyramid,
gable), one **team-tint** material channel, plus **emissive** faces for glow.

| Axis | 🌌 Astromancers (Protoss-leaning) | ⚙️ Hollowmen (Terran-leaning) |
|---|---|---|
| Construction fiction | **Grown / warped in**, hovers off the ground | **Built / assembled**, bolted to the ground |
| Primary forms | Faceted crystal, tapered towers, domes, rings, octagons | Stacked boxes, gable/sawtooth roofs, cylinders (tanks/stacks) |
| Symmetry | Symmetric, ceremonial, jewel-like | Asymmetric, industrial, "assembled in a yard" |
| Silhouette tell | A floating glowing core + a crowning spire/crystal | A roof tower/turret + stacks/antennae, a blast door |
| Shell material | Pale grown ivory/bone (`~#EEE9DB`) | Gunmetal steel (`~#7C8490`), plated highlights |
| Trim / accent | Gold (`#EACC7E`) ceremonial edges | Hazard amber/orange (`#D69E36`) stripes |
| Energy / glow (emissive) | Violet core (`#D296FF`) + teal seams (`#A0FFF6`) | Cyan readouts (`#6EC4E0`) + amber vents (`#FAC460`) |
| Team-tint channel | The crystal core + trim | The window band + door frame |
| Grounding | Hovers; under-shadow with a **gap**, faint glow plinth | Planted; tarmac pad, hazard skirt, contact shadow |
| Defense | Mostly unarmed; lean on Citadel/Wards | **Every building has a built-in gun** (visible barrel) |
| Motion (later) | Slow bob / counter-rotating rings; fortress class lifts off | Static; door/vent/turret anims, smokestack puffs |

**Scale and footprint.** Three footprint tiers, shared by both factions so the
base layout reads consistently: **Small** ~5x5 (defense, supply), **Medium**
~8x8 (production, economy), **Large** ~11x11 (HQ, superweapon). Height encodes
importance: HQ and tech spires are the tallest things in a base.

**The kit of parts (what to author once, reuse everywhere).**
- Astromancers: `crystal` (prism + tapered tip), `dome` (wide frustum), `ring`
  (octagon prism), `plinth` (hover root: inverted frustum), gold `rim` band,
  emissive `core` and `seam`.
- Hollowmen: `plate-box`, `gable`/`sawtooth` roof, `tank`/`stack` (cylinder),
  `blast-door`, `antenna`, `turret` (box + barrel), hazard `skirt` stripe,
  emissive `window-band` and `vent`.

---

## 3. Astromancers - structure roster

Grown, hovering, unarmed (mostly). Ivory shells, gold trim, violet core, teal
seams. Each hovers over a contact shadow with a visible air gap.

- **Spire (HQ)** - tapered faceted tower on a hanging grown root, crowned by a
  gold spire with a violet core; small shards orbit the base. Tallest building in
  the colony, the tech root.
- **Reliquary** (Mana income) - two clean posts under a gold yoke cradling a
  floating violet octahedral relic that pulses; the silhouette is "a jewel held
  up to the sky."
- **Sanctum** (production) - broad grown hall, gold cornice, a low dome, a
  central crystal; glowing teal archway (the warp-in gate) and window slits, with
  two hovering side pods.
- **Citadel** (defense temple) - stepped fortress with a gold-domed keep and four
  teal corner battle-horns (its guns). This is the one Astromancer structure that
  fights, and it later uproots into the **Flying Fortress**.
- **Sky-Cradle** - an open bowl-cradle ringed by gold-tipped guard spires,
  growing a half-formed flying-castle pod suspended in the cup; births the
  capital **Sky-Bastion**.

*(Glue/economy stubs to add as the kit firms up: a Ward/seeing-eye detector and a
ley-node Mana booster reuse the crystal + ring parts.)*

---

## 4. Hollowmen - structure roster

Built, grounded, **self-defending** (every building shows a barrel). Gunmetal
shells, plated highlights, hazard amber stripes, cyan readouts, amber vents.

- **Command HQ** - broad armored block with angled corner armor, a raised control
  tower (cyan window band) and a roof turret; antenna mast, hazard base skirt,
  blast door. The base's anchor and detector (radar).
- **Refinery** (materials income) - a plant hall with a gable roof, two storage
  tanks (one hazard-banded), connecting pipes, and a glowing amber vent stack.
- **Reactor** (Power) - squat containment with a banded dome and a cyan core,
  flanked by two flared cooling stacks; glowing amber body vents. Gates
  production and upkeep.
- **Factory / Barracks** (production) - wide sawtooth-roofed hangar, a hazard-
  framed roll-up door, twin smokestacks, a crane arm, and a corner gun. This is
  the structure the sim already spawns (the current `Barracks`).
- **Null Pylon** (anti-caster defense) - armored hex base and a tall emitter mast
  ringed by violet null-field coils, with a base gun. Projects the magic-denial
  zone that switches off enemy casters.
- **Missile Silo** (superweapon, to model next) - a hardened bunker with hazard
  blast doors and a recessed nuke tube; reuses the plate-box + skirt + door kit.

---

## 5. State, motion, and production notes

**Readable states (both factions).**
- *Construction:* Astromancers grow upward from the root (scale/warp-in);
  Hollowmen raise a girder scaffold then plate over it.
- *Powered/working:* emissive seams/vents lit when active, dimmed when unpowered
  (Hollowmen) or low-Mana (Astromancers).
- *Damaged:* Hollowmen vent smoke and show scorched plates; Astromancers' core
  flickers and the hover dips.
- *Selected:* the existing ground selection ring, tinted by team.

**How this maps to the renderer.** Each building is a `Vec<UnitVertex>` composed
from the kit primitives (mirror the Python concept's box/prism/frustum/pyramid
in `gfx.rs`). The `color.a` team channel stays reserved for the trim/core/window;
add an **emissive** path (a vertex flag, or a tiny additive pass) for the glow
faces so the violet cores and amber vents read in fog. Keep poly counts low
(the concept buildings are ~30-80 quads each, which is the right budget).

**Build order to validate the language:** model **Factory/Barracks** (Hollowmen,
already in the sim) and **Spire (HQ)** (Astromancers) first; they exercise the
full kit (boxes + door + turret + stripe vs crystal + dome + core + hover), and
prove the two silhouettes read against each other on the same battlefield.
