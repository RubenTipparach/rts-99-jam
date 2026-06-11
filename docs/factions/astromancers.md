# Astromancers - faction concept

Companion deep-dive to the roster in [`../factions.md`](../factions.md) and
the lore in [`../story.md`](../story.md). This is the *concept*: who the
Astromancers are on screen, what rules their art and effects follow, and what
fantasy the player lives when they pick them.

**One line:** *wizard-scholars as an atomic-age great power* - a hidden
academic order that broke its bloodline's lock, out-invented the world that
tried to burn it, and now grows porcelain cities on the irradiated moons it
conquered. Not robed mystics in a fantasy forest: a spacefaring civilization
whose science happens to be spellcraft. **SCIENTIA EST MAGIA.**

---

## Design pillars

1. **Grown, not built.** Nothing is welded and nothing is bolted. Structures
   are *cultivated* - an Acolyte tends a site and the building rises like a
   shoot, sheathed in a smooth ceramic shell. Where the Hollowmen show seams,
   rivets, and scaffolds, Astromancer surfaces are seamless, curved, and
   slightly asymmetric, the way grown things are.
2. **Everything floats.** Buildings hover a hand's width off their pads; the
   Acolyte's limbs orbit its body without joints; capital structures uproot
   and fly. Contact with the ground is a choice, not a necessity. (In-game:
   hover bob on workers, floating-joint rigs, and the Flying Fortress /
   Sky-Bastion late tier.)
3. **Few, precious, shielded.** Every unit is an educated citizen of a small
   nation; none are disposable. High cost, high power, regenerating shields,
   and the player feel of *guarding* an army rather than spending one. Losing
   a Golem should sting like losing a building.
4. **Radiation is home.** The bomb that was meant to end them became their
   element. Irradiated ground heals them and feeds their casting: visually,
   their colonies bloom exactly where the world looks most poisoned.
5. **Knowledge as ordnance.** Magic here is engineering: metered, industrial,
   repeatable. Casting looks like *operating an instrument*, not pleading
   with the heavens - circles, calipers, and lenses, never lightning from
   fingertips.

## Visual language

**Silhouette.** Tapering verticals and orbital rings. The Spire is the type
specimen: a slender grown tower with rings of floating masonry circling it.
Units read as *figure plus satellites* - a core body with floating shoulder
plates, halo fragments, or orbiting foci. No wheels, no treads, no exhausts.

**Materials.**
- *Grown shell* - pale porcelain-bone ceramic, the body of every structure
  and heavy unit; smooth, matte, softly rounded (the meshes' light greys).
- *Aether* - the working fluid of their civilization: cyan-white light seen
  through slots, seams, and lenses (the in-game crystal/beam cyan,
  `[0.55, 0.90, 1.0]`).
- *Auric filigree* - thin brass-gold inlay tracing the shell like circuitry
  or illuminated-manuscript borders; their "wiring".
- *Team color* - worn, not painted: banner sashes, window glass, and shield
  tint carry the player color (mesh team-weight surfaces).

**Palette.** Porcelain ivory + deep indigo shadow + aether cyan + auric gold,
with team color as the accent. Warm light from within, cold light from
without.

**Iconography.** The crest of Philosophia: an eight-pointed star caged in an
orbital ring (the branding mark). Numbers and script appear as engraved Latin
in gold; the Astromancers are the only faction that decorates.

## Effects language (fx)

- **Aether weapons** are *coherent light with mass*: a beam that blooms into
  petals at the impact point, never a muzzle flash. Cyan-white core, soft
  bloom, point light. (The mining laser and crystal glow already speak this.)
- **Shields** pop as a soap-film shimmer: a brief iridescent shell flash on
  hit, and a slow pearl glow while regenerating.
- **Growth** (construction) is the inverse of the Hollowmen's weld sparks:
  motes of light spiral *inward* and the shell rises out of the pad; the
  finished building exhales a ring of light when it comes alive (floodlights
  on).
- **Radiation healing** reads as green-gold fireflies rising off the ground
  around units standing in hot zones.
- **Death** is dignified: shields fail (film bursts), then the shell cracks
  with light before the body collapses; no greasy fire, no debris field.

## Sound & voice (notes)

Glass, choir, and geiger. Structures hum a low choral drone; orders are
confirmed in clipped scholar's Latin over a faint chime; weapons sound like
struck crystal and tearing silk rather than gunpowder. The geiger-counter
crackle, the sound everyone else fears, plays *calm* in their bases - it is
their birdsong.

## The player fantasy (match arc)

- **Opening - the seed.** A Spire and a handful of Acolytes on a leveled pad.
  Workers drift between crystal nodes; the first Sanctum grows. The base
  looks like a shrine taking root.
- **Midgame - the order militant.** Magus lines behind Shield-Knights, a
  Reliquary trickling mana, Wards rooting the frontier. Each engagement is
  fought to preserve units: pull shields back, rotate, regenerate, re-engage.
  The army is a faculty, not a mob.
- **Lategame - the city walks.** Citadels lift off; the colony itself becomes
  the offensive. A Sky-Bastion crossing the map with its escort is the
  faction's exclamation mark: the place you built is now the army.

**Strengths to feel:** quality, recovery (shields and healing make retreats
profitable), terrain inversion (radiation zones are *their* high ground).
**Weaknesses to feel:** slow to mass, expensive to replace, brittle against
denial (Hollowmen null-fields switch the whole identity off).

## What exists in-game today

| Concept | In-game now (`apps/client/src/gfx.rs`) |
|---|---|
| Spire (HQ) | `hq_mesh_astro` - grown tower, floating rings |
| Sanctum (production) | `barracks_mesh_astro` |
| Ward (defense) | turret mesh (shared shape, Astromancer name) |
| Acolyte (worker) | `acolyte_mesh` - floating-joint robot, hover bob |
| Aether fx | mining beam, crystal glow, point-light fx |

**Next assets that would pay off most** (in concept order): the Magus
(caster infantry with an orbiting focus), the Shield-Knight (the staple
frontline; first unit to show the soap-film shield flash), and the Reliquary
(the mana building - a floating reliquary casket between two pylons; gives
the base its second vertical).
