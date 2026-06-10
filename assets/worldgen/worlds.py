#!/usr/bin/env python3
"""The world catalog: texture archetypes and per-world generation parameters.

Each battlefield is one of the Solar System bodies the player can fight over.
Many of them share a look (airless cratered rock, dirty ice), so they are grouped
into a handful of texture *archetypes*; a per-world tint, seed and terrain recipe
then make each one distinct. A few worlds (Io, Europa, Titan, Enceladus, Triton,
Pluto, Mars) are visually unique and get their own archetype.

Palettes are hand-tuned to match real spacecraft imagery (see `nasa` on each
world and `docs/worldgen/nasa_references.json`): LROC for the Moon, Dawn for Vesta
and Ceres, Galileo for Io / Europa / Ganymede / Callisto, Cassini for the Saturn
moons and Titan, Voyager 2 for the Uranus moons and Triton, and New Horizons for
Pluto.

This module is data only and presentation-side: nothing here touches the
deterministic sim (CLAUDE.md, "Two worlds, one wall").
"""

import math


# --------------------------------------------------------------------------- #
# texture archetypes: the shared "material kits"
# --------------------------------------------------------------------------- #
# Each palette has: low (basins / shadow / mare), mid (typical regolith),
# high (sunlit highs / fresh ice), accent (the defining splash of colour:
# ejecta rays, lineae, lava, salts), and dark (flecks). Tiles are generated
# per archetype in textures.py; render.py shades terrain from the same palette.
ARCHETYPES = {
    # Airless grey rock: lunar / large-asteroid / Uranian-moon regolith. Kept a
    # touch warm (neutral-to-tan) so it reads as bare rock next to the cool ices.
    "regolith_grey": {
        "low": (60, 58, 56), "mid": (124, 119, 112), "high": (180, 174, 164),
        "accent": (208, 203, 195), "dark": (44, 42, 40), "scale": 5.0,
    },
    # Dark carbonaceous rock: Ceres / Umbriel / centaurs. Bright salt accent.
    "regolith_dark": {
        "low": (30, 29, 30), "mid": (62, 60, 60), "high": (100, 97, 94),
        "accent": (198, 203, 209), "dark": (20, 19, 20), "scale": 5.0,
    },
    # Dirty cratered ice: Callisto / Rhea / Dione / Iapetus. Brown-grey lowlands
    # but distinctly icy (cool, bright) highs, so it never reads as bare rock.
    "dirty_ice": {
        "low": (78, 68, 58), "mid": (130, 126, 124), "high": (188, 196, 204),
        "accent": (216, 224, 234), "dark": (54, 47, 40), "scale": 5.5,
    },
    # Grooved ice: Ganymede / Ariel / Miranda. Dark ancient terrain cut by bright
    # bluish sulci. Cooler than the rocky greys to sell the ice-shell look.
    "grooved_ice": {
        "low": (78, 80, 90), "mid": (126, 132, 144), "high": (190, 200, 214),
        "accent": (214, 226, 240), "dark": (56, 58, 68), "scale": 6.0,
    },
    # Brilliant fresh ice: Enceladus. White with blue fracture accents.
    "bright_ice": {
        "low": (150, 170, 186), "mid": (208, 217, 225), "high": (238, 244, 249),
        "accent": (120, 165, 198), "dark": (132, 150, 168), "scale": 4.5,
    },
    # UNIQUE: Europa. Bright tan ice scored by red-brown lineae and chaos.
    "europa_ice": {
        "low": (150, 150, 162), "mid": (212, 206, 196), "high": (238, 233, 224),
        "accent": (156, 98, 70), "dark": (120, 118, 128), "scale": 4.0,
    },
    # UNIQUE: Mars. Rusty dust and basalt, with bright polar ice.
    "mars_rust": {
        "low": (92, 52, 36), "mid": (162, 98, 64), "high": (202, 150, 110),
        "accent": (224, 224, 230), "dark": (60, 38, 30), "scale": 4.5,
    },
    # UNIQUE: Io. Sulfur yellows with red pyroclastics; black silicate stays in
    # the speckle/lava channels only, so the plains read yellow, not charred.
    "io_sulfur": {
        "low": (150, 122, 50), "mid": (212, 188, 86), "high": (232, 220, 168),
        "accent": (214, 96, 40), "dark": (58, 46, 26), "scale": 4.0,
    },
    # UNIQUE: Titan. Orange organic haze, dark dunes, dark hydrocarbon lakes.
    "titan_haze": {
        "low": (84, 54, 30), "mid": (168, 116, 64), "high": (198, 158, 104),
        "accent": (40, 42, 54), "dark": (60, 38, 22), "scale": 4.5,
    },
    # UNIQUE: Triton. Pinkish nitrogen ice, cantaloupe terrain, dark plume streaks.
    "triton_ice": {
        "low": (150, 122, 122), "mid": (206, 180, 170), "high": (230, 214, 206),
        "accent": (84, 66, 70), "dark": (132, 104, 104), "scale": 5.0,
    },
    # UNIQUE: Pluto. Tan tholins, bright nitrogen plains, red-brown uplands.
    "pluto_tholin": {
        "low": (110, 78, 60), "mid": (172, 132, 100), "high": (226, 208, 180),
        "accent": (134, 76, 54), "dark": (84, 58, 44), "scale": 5.0,
    },
    # UNIQUE: Earth. Green continents, rock highlands, snow peaks (oceans/lakes
    # are drawn as liquid, not a material slot).
    "earth": {
        "low": (96, 132, 72), "mid": (74, 112, 58), "high": (120, 112, 96),
        "accent": (236, 240, 244), "dark": (58, 86, 48), "scale": 4.0,
    },
}


def _terrain(**kw):
    """Terrain recipe with sensible defaults; override only what differs."""
    base = dict(
        seed=1, relief=1.0, warp=0.6, roughness=1.0,
        crater_density=1.0, crater_min=0.012, crater_max=0.06, smoothness=0.0,
        grooves=0.0, groove_dir=0.0, rifts=0.0, dunes=0.0, dune_dir=0.0,
        calderas=0.0, cantaloupe=0.0, plains=0.0, ridge=0.0, dichotomy=0.0,
    )
    base.update(kw)
    return base


def _hazard(kind, intensity=0.6, **kw):
    h = dict(kind=kind, intensity=intensity)
    h.update(kw)
    return h


# --------------------------------------------------------------------------- #
# the worlds (declared in the order the request listed them)
# --------------------------------------------------------------------------- #
# tint is a per-channel multiplier on the archetype palette (1.0 = neutral) so
# sibling worlds sharing an archetype still read differently; bright nudges the
# whole thing lighter/darker.
WORLDS = [
    dict(
        key="moon", name="Luna (Moon)", archetype="regolith_grey",
        tint=(1.0, 1.0, 1.0), bright=1.0,
        terrain=_terrain(seed=11, relief=0.9, crater_density=1.6,
                         crater_min=0.01, crater_max=0.09),
        hazards=[_hazard("mare", 0.5), _hazard("rays", 0.6)],
        nasa="Lunar Reconnaissance Orbiter (LROC)",
        blurb="Heavily cratered grey regolith; dark basaltic maria, bright ray craters.",
    ),
    dict(
        key="ceres", name="Ceres", archetype="regolith_dark",
        tint=(1.02, 1.0, 0.98), bright=1.05,
        terrain=_terrain(seed=23, relief=0.8, crater_density=1.4),
        hazards=[_hazard("brine", 0.8, note="Occator faculae"),
                 _hazard("rays", 0.3)],
        nasa="Dawn (Framing Camera)",
        blurb="Dark carbonaceous dwarf planet with brilliant salt deposits (faculae).",
    ),
    dict(
        key="vesta", name="Vesta", archetype="regolith_grey",
        tint=(1.06, 1.0, 0.9), bright=1.02,
        terrain=_terrain(seed=31, relief=1.2, crater_density=1.3,
                         crater_max=0.11, rifts=0.25, groove_dir=0.2),
        hazards=[_hazard("scarp", 0.4, note="Rheasilvia basin troughs"),
                 _hazard("rays", 0.4)],
        nasa="Dawn (Framing Camera)",
        blurb="Battered basaltic protoplanet; a giant south-polar impact and equatorial troughs.",
    ),
    dict(
        key="mars", name="Mars", archetype="mars_rust",
        tint=(1.0, 1.0, 1.0), bright=1.0,
        terrain=_terrain(seed=43, relief=0.75, crater_density=0.6,
                         dunes=0.7, dune_dir=0.5, rifts=0.4, smoothness=0.15),
        hazards=[_hazard("dust_storm", 0.7), _hazard("frost", 0.5, note="polar CO2/H2O ice"),
                 _hazard("scarp", 0.5, note="Valles Marineris")],
        nasa="Viking / Mars Reconnaissance Orbiter",
        blurb="Rust-red dust and basalt, vast canyons, dune seas and bright polar caps.",
    ),
    dict(
        key="callisto", name="Callisto", archetype="dirty_ice",
        tint=(0.9, 0.88, 0.86), bright=0.82,
        terrain=_terrain(seed=51, relief=0.8, crater_density=2.2,
                         crater_min=0.008, crater_max=0.10, smoothness=0.0),
        hazards=[_hazard("radiation", 0.4), _hazard("rays", 0.6)],
        nasa="Galileo (SSI)",
        blurb="The most cratered world known: dark dirty ice, saturated with bright craters.",
    ),
    dict(
        key="ganymede", name="Ganymede", archetype="grooved_ice",
        tint=(1.0, 1.0, 1.02), bright=1.0,
        terrain=_terrain(seed=61, relief=1.0, crater_density=1.0,
                         grooves=0.8, groove_dir=0.6, rifts=0.3),
        hazards=[_hazard("radiation", 0.5), _hazard("ice_rift", 0.4)],
        nasa="Galileo (SSI) / Juno (JunoCam)",
        blurb="Largest moon: dark cratered terrain split by bright grooved sulci, icy below.",
    ),
    dict(
        key="europa", name="Europa", archetype="europa_ice",
        tint=(1.0, 1.0, 1.0), bright=1.06,
        terrain=_terrain(seed=71, relief=0.35, crater_density=0.12,
                         smoothness=0.85, rifts=0.9, grooves=0.5, groove_dir=1.1),
        hazards=[_hazard("ice_rift", 0.9, note="reddish lineae"),
                 _hazard("chaos", 0.7), _hazard("radiation", 0.7)],
        nasa="Galileo (SSI)",
        blurb="Smooth young ice shell laced with red-brown lineae and chaos terrain.",
    ),
    dict(
        key="io", name="Io", archetype="io_sulfur",
        tint=(1.0, 1.0, 1.0), bright=1.0,
        terrain=_terrain(seed=83, relief=0.7, crater_density=0.0,
                         smoothness=0.6, calderas=0.9),
        hazards=[_hazard("lava", 0.95, note="Loki, Pele paterae"),
                 _hazard("radiation", 0.9), _hazard("plume", 0.6)],
        nasa="Galileo (SSI) / Voyager 1",
        blurb="The most volcanic body: sulfur plains, black lava lakes, eruption plumes.",
    ),
    dict(
        key="titan", name="Titan", archetype="titan_haze",
        tint=(1.0, 1.0, 1.0), bright=1.0,
        terrain=_terrain(seed=97, relief=0.5, crater_density=0.1,
                         smoothness=0.55, dunes=0.7, dune_dir=0.1, rifts=0.2),
        hazards=[_hazard("hydro_lake", 0.95, note="Kraken/Ligeia methane seas"),
                 _hazard("dust_storm", 0.28, note="organic haze"),
                 _hazard("cryogeyser", 0.2)],
        nasa="Cassini (ISS/RADAR/VIMS) / Huygens",
        blurb="Hazy orange organics: methane seas, vast dune fields, thick atmosphere.",
    ),
    dict(
        key="enceladus", name="Enceladus", archetype="bright_ice",
        tint=(1.0, 1.0, 1.0), bright=1.04,
        terrain=_terrain(seed=109, relief=0.45, crater_density=0.3,
                         smoothness=0.7, rifts=0.6),
        hazards=[_hazard("cryogeyser", 0.95, note="south-polar tiger stripes"),
                 _hazard("ice_rift", 0.7, note="sulci")],
        nasa="Cassini (ISS)",
        blurb="Dazzling fresh ice; south-polar tiger-stripe fractures vent icy jets.",
    ),
    dict(
        key="triton", name="Triton", archetype="triton_ice",
        tint=(1.0, 1.0, 1.0), bright=1.0,
        terrain=_terrain(seed=127, relief=0.5, crater_density=0.15,
                         smoothness=0.6, cantaloupe=0.85, plains=0.3),
        hazards=[_hazard("cryogeyser", 0.7, note="nitrogen plumes"),
                 _hazard("frost", 0.4)],
        nasa="Voyager 2 (ISS)",
        blurb="Pinkish nitrogen ice in cantaloupe terrain; dark wind-blown geyser streaks.",
    ),
    dict(
        key="rhea", name="Rhea", archetype="dirty_ice",
        tint=(1.08, 1.08, 1.1), bright=1.12,
        terrain=_terrain(seed=131, relief=0.8, crater_density=1.8,
                         crater_max=0.09),
        hazards=[_hazard("rays", 0.6), _hazard("scarp", 0.3, note="ice cliffs")],
        nasa="Cassini (ISS)",
        blurb="Bright, saturated-cratered water ice with wispy fracture cliffs.",
    ),
    dict(
        key="iapetus", name="Iapetus", archetype="dirty_ice",
        tint=(1.0, 0.96, 0.9), bright=0.95,
        terrain=_terrain(seed=139, relief=1.1, crater_density=1.6,
                         ridge=0.9, dichotomy=0.85),
        hazards=[_hazard("ridge", 0.9, note="equatorial ridge"),
                 _hazard("dichotomy", 0.85, note="Cassini Regio dark side")],
        nasa="Cassini (ISS)",
        blurb="Two-faced moon: jet-black leading side, bright ice, a towering equatorial ridge.",
    ),
    dict(
        key="dione", name="Dione", archetype="dirty_ice",
        tint=(1.06, 1.07, 1.1), bright=1.08,
        terrain=_terrain(seed=149, relief=0.8, crater_density=1.5,
                         rifts=0.5),
        hazards=[_hazard("scarp", 0.7, note="wispy ice cliffs / chasmata"),
                 _hazard("rays", 0.4)],
        nasa="Cassini (ISS)",
        blurb="Cratered ice crossed by bright wispy cliffs (tectonic chasmata).",
    ),
    dict(
        key="titania", name="Titania", archetype="regolith_grey",
        tint=(0.92, 0.9, 0.92), bright=0.86,
        terrain=_terrain(seed=151, relief=1.0, crater_density=1.2,
                         rifts=0.6, groove_dir=0.9),
        hazards=[_hazard("scarp", 0.7, note="Messina Chasmata"),
                 _hazard("rays", 0.3)],
        nasa="Voyager 2 (ISS)",
        blurb="Largest Uranian moon: grey ice-rock cut by enormous fault canyons.",
    ),
    dict(
        key="oberon", name="Oberon", archetype="regolith_grey",
        tint=(0.9, 0.85, 0.84), bright=0.8,
        terrain=_terrain(seed=157, relief=1.1, crater_density=1.5,
                         crater_max=0.1),
        hazards=[_hazard("dark_floor", 0.7, note="dark crater-floor deposits"),
                 _hazard("scarp", 0.4)],
        nasa="Voyager 2 (ISS)",
        blurb="Old cratered surface with mysterious dark material pooled on crater floors.",
    ),
    dict(
        key="umbriel", name="Umbriel", archetype="regolith_dark",
        tint=(0.92, 0.92, 0.95), bright=0.78,
        terrain=_terrain(seed=163, relief=0.8, crater_density=1.4,
                         smoothness=0.1),
        hazards=[_hazard("rays", 0.2, note="bright Wunda ring"),
                 _hazard("radiation", 0.2)],
        nasa="Voyager 2 (ISS)",
        blurb="Darkest, most uniform Uranian moon; one bright ring (Wunda) stands out.",
    ),
    dict(
        key="ariel", name="Ariel", archetype="grooved_ice",
        tint=(1.06, 1.07, 1.1), bright=1.1,
        terrain=_terrain(seed=167, relief=0.9, crater_density=0.7,
                         smoothness=0.3, rifts=0.85, grooves=0.6, groove_dir=1.3),
        hazards=[_hazard("ice_rift", 0.8, note="graben valleys"),
                 _hazard("scarp", 0.5)],
        nasa="Voyager 2 (ISS)",
        blurb="Brightest Uranian moon: young ice resurfaced by deep rift valleys.",
    ),
    dict(
        key="miranda", name="Miranda", archetype="grooved_ice",
        tint=(0.98, 0.98, 1.0), bright=0.96,
        terrain=_terrain(seed=173, relief=1.5, crater_density=0.9,
                         grooves=0.9, groove_dir=0.4, rifts=0.9),
        hazards=[_hazard("scarp", 0.95, note="Verona Rupes, ~20 km cliff"),
                 _hazard("ice_rift", 0.7, note="coronae")],
        nasa="Voyager 2 (ISS)",
        blurb="Patchwork of coronae and the Solar System's tallest cliff; chaotic ridges.",
    ),
    dict(
        key="pluto", name="Pluto", archetype="pluto_tholin",
        tint=(1.0, 1.0, 1.0), bright=1.0,
        terrain=_terrain(seed=181, relief=1.0, crater_density=0.5,
                         plains=0.7, rifts=0.3, smoothness=0.2),
        hazards=[_hazard("plains", 0.85, note="Sputnik Planitia nitrogen glaciers"),
                 _hazard("frost", 0.4), _hazard("cryogeyser", 0.3, note="Wright Mons")],
        nasa="New Horizons (LORRI/Ralph)",
        blurb="Tan tholins and red-brown uplands beside a bright nitrogen-ice heart.",
    ),
    dict(
        key="chiron", name="Chiron", archetype="regolith_dark",
        tint=(0.95, 0.97, 1.02), bright=0.84,
        terrain=_terrain(seed=191, relief=0.9, crater_density=1.0,
                         crater_max=0.12),
        hazards=[_hazard("outgassing", 0.7, note="cometary sublimation jets"),
                 _hazard("rays", 0.2)],
        nasa="ground-based / Hubble (centaur, comet 95P)",
        blurb="A dark icy-rock centaur that flares with cometary activity near perihelion.",
    ),
    dict(
        key="earth", name="Earth", archetype="earth",
        tint=(1.0, 1.0, 1.0), bright=1.0,
        terrain=_terrain(seed=200, relief=1.0, crater_density=0.15,
                         smoothness=0.4, rifts=0.2),
        hazards=[_hazard("ocean", 0.9, note="seas and coasts"),
                 _hazard("river", 0.6, note="rivers and lakes"),
                 _hazard("dust_storm", 0.3, note="weather systems")],
        nasa="Landsat / Blue Marble (NASA Earth Observatory)",
        blurb="Home: blue oceans, green continents, mountain ranges, lakes and rivers.",
    ),
]


def by_key(key):
    for w in WORLDS:
        if w["key"] == key:
            return w
    raise KeyError(key)


def palette(world):
    return ARCHETYPES[world["archetype"]]


def groups():
    """Map archetype -> list of world keys (for docs / reporting)."""
    out = {}
    for w in WORLDS:
        out.setdefault(w["archetype"], []).append(w["key"])
    return out


# A light sanity check when run directly.
if __name__ == "__main__":
    print(f"{len(WORLDS)} worlds across {len(set(w['archetype'] for w in WORLDS))} archetypes")
    for arch, keys in groups().items():
        assert arch in ARCHETYPES, arch
        print(f"  {arch:14s} {', '.join(keys)}")
    for w in WORLDS:
        assert set(w["tint"]) and w["bright"] > 0
        assert "seed" in w["terrain"]
        _ = math.sin(w["terrain"]["groove_dir"])  # dirs are radians
    print("catalog ok")
