//! Wire types shared across the engine.
//!
//! In a deterministic-lockstep RTS, the network transports **only commands**
//! (player/AI intents), never world state — see
//! `docs/architecture/03-networking-lockstep.md`. Keeping these types in their
//! own crate lets `sim`, `net`, `replay`, and `ai` agree on the format without
//! depending on each other.
//!
//! Everything here is fixed-point (no floats), so a command means the same
//! thing on every machine.

#![forbid(unsafe_code)]

use math::Vec3;

/// Identifies a player (human or AI) in a match.
pub type PlayerId = u16;

/// A single player/AI intent for one simulation tick.
///
/// Milestone 0 has just enough to exercise the deterministic core (spawn, set
/// velocity, despawn). Real RTS commands (move, attack, build, …) layer on later
/// — see the roadmap, `docs/architecture/10-roadmap-testing.md`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Command {
    /// Create a unit owned by `owner` at `pos` with a per-tick `vel`.
    Spawn {
        owner: PlayerId,
        pos: Vec3,
        vel: Vec3,
    },
    /// Replace the velocity of the live unit in slot `entity_index`.
    SetVelocity { entity_index: u32, vel: Vec3 },
    /// Remove the live unit in slot `entity_index`.
    Despawn { entity_index: u32 },
}
