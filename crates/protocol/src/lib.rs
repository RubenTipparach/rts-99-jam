//! Wire types shared across the engine.
//!
//! The network transports **only commands** (player/AI intents), never world
//! state - see `docs/architecture/03-networking-lockstep.md`. Everything here is
//! fixed-point (no floats), so a command means the same thing on every machine.

#![forbid(unsafe_code)]

use math::Fx;

/// Identifies a player (0 = local player, 1 = enemy, …).
pub type PlayerId = u16;

/// Kinds of mobile unit.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum UnitKind {
    Infantry,
    /// Builder/harvester (Astromancer Acolyte, Hollowmen Engineer): mobile but
    /// non-combatant. Gathers materials and raises structures.
    Worker,
    /// Heavy assault unit (Astromancer Golem, Hollowmen War-Mech): slow, tanky,
    /// hits hard. Costs ore and carbon.
    Heavy,
    /// Astromancer evocation caster: short reach, burns through packed
    /// infantry fast, fragile. Faction-locked.
    Pyromancer,
    /// Astromancer tempest caster: long-reach bolts that hit hard but slowly.
    /// Faction-locked.
    Stormcaller,
    /// Hollowmen light recon mech: very fast, cheap, weak. Faction-locked.
    Hound,
    /// Hollowmen missile mech: long-reach fire support with real armor.
    /// Faction-locked.
    Javelin,
}

/// Kinds of structure.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum BuildingKind {
    /// Headquarters (Astromancer Spire, Hollowmen Command HQ): the tech root.
    /// Trains workers and is the workers' deposit point. Every player starts
    /// with one; building another founds an expansion.
    Hq,
    Barracks,
    /// Defensive emplacement: immobile, auto-fires on nearby enemies. Costs ore
    /// and carbon.
    Turret,
    /// Supply depot: raises the owner's unit cap (the HQ provides a base
    /// amount; each depot adds more). Builds nothing and has no weapon.
    Supply,
    /// Astromancer storm coil: pricier than a Ward but strikes from much
    /// further out. Faction-locked.
    StormWard,
    /// Hollowmen blockhouse: cheap, tough, short-reach rapid fire.
    /// Faction-locked.
    Bunker,
    /// Astromancer caster college: trains the Pyromancer and Stormcaller.
    Athenaeum,
    /// Astromancer forge (tier 2 tech shell). Faction-locked.
    Crucible,
    /// Astromancer research dome (tech shell). Faction-locked.
    Conservatory,
    /// Astromancer air roost (tech shell). Faction-locked.
    Aerie,
    /// Astromancer superweapon site (tech shell). Faction-locked.
    LeyNexus,
    /// Hollowmen mech line: trains the Hound and Javelin.
    MachineShop,
    /// Hollowmen munitions plant (tech shell). Faction-locked.
    Arsenal,
    /// Hollowmen detection mast (tech shell). Faction-locked.
    RadarArray,
    /// Hollowmen air pad (tech shell). Faction-locked.
    Starport,
    /// Hollowmen power plant (tech shell). Faction-locked.
    FusionReactor,
    /// Hollowmen capital yard (tech shell). Faction-locked.
    Drydock,
    /// Hollowmen nuke site (tech shell). Faction-locked.
    MissileSilo,
}

/// Harvestable resource nodes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ResourceKind {
    /// Ore: shiny crystals that erupt from the ground.
    Ore,
    /// Carbon: a gas geyser.
    Carbon,
}

/// A single player/AI intent for one simulation tick. Units/buildings are
/// referenced by their slot index (from the world snapshot). One command per
/// entity keeps this `Copy` and trivial to serialize.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Command {
    /// Scenario setup: place a unit.
    SpawnUnit {
        owner: PlayerId,
        kind: UnitKind,
        x: Fx,
        y: Fx,
    },
    /// Scenario setup: place a building.
    SpawnBuilding {
        owner: PlayerId,
        kind: BuildingKind,
        x: Fx,
        y: Fx,
    },
    /// Scenario setup: place a neutral resource node.
    SpawnResource { kind: ResourceKind, x: Fx, y: Fx },
    /// Send a worker to harvest a resource node (mine, then return to deposit).
    Harvest { unit: u32, node: u32 },
    /// Send a worker to repair a damaged friendly building (costs ore per
    /// hit point restored).
    Repair { unit: u32, target: u32 },
    /// Send a worker to construct a building at a point (costs ore on arrival).
    Build {
        unit: u32,
        kind: BuildingKind,
        x: Fx,
        y: Fx,
    },
    /// Move to a point (no auto-engage on the way).
    Move { unit: u32, x: Fx, y: Fx },
    /// Move to a point, attacking any enemy encountered.
    AttackMove { unit: u32, x: Fx, y: Fx },
    /// Attack a specific entity (chase it).
    Attack { unit: u32, target: u32 },
    /// Queue one unit of `kind` for production at a building.
    Train { building: u32, kind: UnitKind },
    /// Hold position.
    Stop { unit: u32 },
    /// Set where a building's new units gather.
    SetRally { building: u32, x: Fx, y: Fx },
}
