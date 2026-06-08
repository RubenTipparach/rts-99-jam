# Faction design - units, structures, doctrine

Game-design companion to the lore in [`story.md`](story.md). Five
asymmetric factions for the **ground RTS**. (Space battles are out of scope for
now; "ships" here are the air / heavy tier that shows up in ground fights.)
Origins, philosophies, and slogans live in `story.md`; this file is the roster.

## Economy spine

Two resources, refracted per faction:

- **Materials** (ore / salvage) - builds everything physical. (The sim's existing
  `ore`.)
- **Magic / aether** - powers abilities, shields, beams, and conversions. Each
  faction *relates to magic differently*, and that relationship is the core
  asymmetry:

| Faction | How they get magic |
|---|---|
| Astromancers | Generate it natively (reliquaries, ley-nodes) |
| Hollowmen | None: they deny it (null-fields) and burn allies' bottled magic |
| Rimelings | Soak it from nearby magic-folk; a trickle from ether-collectors |
| Ninefold | Generate it, tied to a lunar / turn cycle |
| Warren | Scavenge it from salvage and the converted dead |

Every faction still needs the RTS basics - a **HQ**, a **builder/worker**, a
**materials extractor**, **production**, and **defense** - so those are listed
even where the flavor is unusual. Items marked *(glue)* are standard-RTS
scaffolding added for completeness, easy to rename or cut.

---

## Astromancers - grown, hovering, maged

*"Knowledge is magic."* Elite and few. Everything hovers, structures are *grown*
rather than built, and units carry regenerating magical shields. Powerful and
expensive, slow to mass. Home turf: irradiated zones, where they heal.

**Ground**
- **Acolyte** *(glue)* - worker; *grows* structures instead of constructing them.
- **Magus** - line caster infantry; aether-bolts.
- **Shield-Knight** - small magic-imbued mech suit with a regenerating shield; the staple frontline.
- **Golem** - heavy grown bruiser; no shield, huge HP.
- **Hover-tank** - fast hovering armor with an arcane cannon.

**Structures** (grown over time)
- **Spire** (HQ) *(glue)* - grows the colony; tech root.
- **Reliquary** - magic income.
- **Sanctum** *(glue)* - unit production.
- **Citadel** - defensive temple; see Flying Fortress.

**Air & heavies**
- **Hover-craft** - skirmisher gunships.
- **Dragon** - tech-outfitted drake with laser breath; air superiority / bomber.
- **Flying Fortress** - a Citadel that lifts off: slow mobile temple-fortress, heavy guns, can re-root elsewhere.

**Signature mechanic - regeneration & radiation.** Units self-shield and
regenerate; in irradiated terrain they heal faster and hit harder. Grown
structures can uproot and relocate (the flying fortress).

---

## Hollowmen - conventional, industrial, anti-magic

*"We make our own power."* Mass-produced conventional war: tanks, mechs, powered
armor, lasers, and nukes. They cannot cast, so their edge is **denial**
(null-fields) plus industrial output and firepower.

**Ground**
- **Engineer** *(glue)* - worker; drives construction rigs.
- **Powered-Armor Trooper** - durable line infantry; laser rifle.
- **Main Battle Tank** - the backbone armor.
- **War-Mech** - heavy walker; anti-everything brawler.
- **Dampener Team** - projects a **null-field**: enemy casting fails inside it.

**Structures**
- **Command HQ** *(glue)*.
- **Refinery** *(glue)* - materials income.
- **Reactor** - power; gates production and upkeep.
- **Factory / Barracks** *(glue)* - vehicle and infantry production.
- **Null Pylon** - area magic-denial (anti-caster zone defense).
- **Missile Silo** - builds **nukes** (superweapon).

**Air & heavies**
- **Gunship** - laser air support.
- **Dropship** *(glue)* - mobility.
- **Capital Warship** - the "big ship"; heavy laser batteries.

**Signature mechanic - nukes & null-fields.** Superweapon nukes for siege;
null-fields and dampeners switch off the enemy's whole magic kit so iron wins.
Cheapest, fastest reinforcements in the game.

---

## Ninefold (cats) - stealth, holograms, glass cannons

*"Made to serve, born to rule."* Few, fragile, and nearly invisible. Stealth on
everything, holographic disguises, snipers, and stealth airpower. Low HP and
armor, but hard to find and hard to finish. Arbiter-era Protoss in spirit.

**Ground**
- **Shade** *(glue)* - cloaked worker / scout.
- **Stalker** - stealth hunter.
- **Sniper** - long-range laser; picks off key targets from cloak.
- **Witch** - lunar caster; debuffs and burst magic.
- **Trickster** - deploys **holographic decoys** and disguises friendly units as something else.

**Structures** (few, high-value, cloak-screened)
- **Moon-court** (HQ) *(glue)*.
- **Coven** - tech / spell research.
- **Holo-Emitter** - cloaks a zone or the base.
- **Shadow-den** *(glue)* - production.

**Air & heavies**
- **Stealth Fighter** - air superiority.
- **Stealth Bomber** - alpha-strike from invisibility.
- **Stealth Carrier** - sleek; cloaks itself *and* nearby units, and deploys units carried inside its frame (Arbiter-like).

**Signature mechanic - stealth & nine lives.** Almost everything is cloaked;
holograms create false targets and disguise units; **detection hard-counters
them**. Key units revive once when killed. Quality over quantity.

---

## Warren (rats) - swarm, tunnels, conversion

*"There are always more of us."* A hivemind swarm: cheap, countless, organic.
Nests link into a tunnel network for instant army redeployment, and the dead
(theirs and yours) become more rats. Zerg-adjacent.

**Ground**
- **Worker-rat** *(glue)* - builder and cheap harvester.
- **Swarm-rat** - throwaway mass infantry.
- **Mega-rat** - the tank tier; huge organic bruiser.
- **Sapper** - tunnels and sabotage.
- **Plague-bomber** - suicide unit; drops a **plague** that converts enemy units and structures into Warren rats.

**Structures** (organic, interconnected)
- **Nest** (HQ) - spawns and grows.
- **Tunnel** - links nests; units move between any linked nests near-instantly (map-wide flanking).
- **Breeder** *(glue)* - production / population.
- **Scrap-gut** - materials from salvage and corpses.

**Air & heavies** (organic)
- **Spore-flyer** - living swarm fighter.
- **Bile-bomber** - organic bomber.
- **Leviathan-rat** - organic capital beast (the "organic ship").

**Signature mechanic - hivemind, tunnels & conversion.** Move whole armies
instantly between linked nests; overwhelm with expendable numbers; convert
casualties into more Warren via plague. Attrition that grows the more it fights.

---

## Rimelings (squid aliens) - parasitic, fissures, hybrid tech-organic

*"All fire is borrowed."* Squid-like, many-legged aliens. They make
almost no magic (a trickle from ether-collectors) and **soak the rest from
nearby magic-folk** (Astromancers, Ninefold, Warren). Hybrid tech-organic, like
the Protoss: energy beams that must be charged, planet-cracking fissure weapons,
and the fastest base-building in the game (structures erupt straight from the
ground).

**Ground**
- **Drone** *(glue)* - worker; erects structures almost instantly from the ground.
- **Spider-mech** - all-terrain, multi-legged walker; the versatile frontline.
- **Fissure-caster** - cracks the ground: line / AoE quakes and eruptions.
- **Beamer** - energy-beam infantry; charge/ammo from ether-collectors.

**Structures** (erupt from the ground, very fast)
- **Hatch-Nexus** (HQ) *(glue)*.
- **Ether-Collector** - the keystone: slow magic generation, recharges beam weapons, refills fighter ammo.
- **Spawning-spire** *(glue)* - production.
- **Fissure-ward** - terrain-cracking defense.

**Air & heavies** (tech-organic hybrid)
- **Fighter** - conventional-style, but **ammo comes from ether-collectors**.
- **Beam-Cruiser** - large, slow, lumbering; its main beam must **charge at an ether-collector** before it fires.
- All Rimeling ships **passively absorb magic from nearby magic-folk units** (Astros, cats, rats), starving casters while fuelling themselves.

**Signature mechanic - soak & fissure.** No native magic economy: drain it off
the enemy's own casters and trickle it from collectors; beams and ammo run on
that stored ether. Fissure weapons reshape the battlefield. Lightning-fast
expansion via instant structures.

---

## Open design questions

- **Resource model:** confirm the two-resource spine (materials + magic), and
  whether magic is one pool or tracked per source.
- **Detection vs stealth:** the Ninefold force every faction to field a detector;
  decide each faction's answer (Hollowmen radar, Astromancer seeing-eye, Rimeling
  sense, etc.).
- **Conversion & soak caps:** Warren plague-conversion and Rimeling magic-soak
  both feed on the enemy and need caps or counters so they cannot snowball
  unstoppably.
- **Jam scope:** likely ship two factions first (Astromancers vs Hollowmen, the
  cleanest grown-vs-manufactured duel), the other three as stretch.
