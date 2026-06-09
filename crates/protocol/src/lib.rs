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
}

/// Kinds of structure.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum BuildingKind {
    Barracks,
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
    /// Move to a point (no auto-engage on the way).
    Move { unit: u32, x: Fx, y: Fx },
    /// Move to a point, attacking any enemy encountered.
    AttackMove { unit: u32, x: Fx, y: Fx },
    /// Attack a specific entity (chase it).
    Attack { unit: u32, target: u32 },
    /// Queue one unit for production at a building.
    Train { building: u32 },
    /// Hold position.
    Stop { unit: u32 },
    /// Set where a building's new units gather.
    SetRally { building: u32, x: Fx, y: Fx },
}
