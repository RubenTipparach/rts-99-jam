//! Headless simulation harness.
//!
//! No window, no renderer — just drive [`sim::World`] from a [`Replay`] and read
//! the resulting state hash. This is what the determinism CI, performance
//! benchmarks, and (later) bot-vs-bot matches run on. See
//! `docs/architecture/10-roadmap-testing.md`.

#![forbid(unsafe_code)]

use math::{Fx, Vec3};
use protocol::Command;
use replay::Replay;
use sim::World;

/// Run a replay to completion and return the final world.
pub fn run_replay(replay: &Replay) -> World {
    let mut world = World::new(replay.seed);
    for turn in &replay.turns {
        world.step(turn);
    }
    world
}

/// Run a replay and return the final state hash. The whole point of the engine:
/// this value must be identical on every platform.
pub fn final_hash(replay: &Replay) -> u64 {
    run_replay(replay).state_hash()
}

#[inline]
fn v(x: i32, y: i32, z: i32) -> Vec3 {
    Vec3::new(Fx::from_int(x), Fx::from_int(y), Fx::from_int(z))
}

#[inline]
fn vr(xn: i64, xd: i64, yn: i64, yd: i64, zn: i64, zd: i64) -> Vec3 {
    Vec3::new(
        Fx::from_ratio(xn, xd),
        Fx::from_ratio(yn, yd),
        Fx::from_ratio(zn, zd),
    )
}

/// A fixed reference scenario that exercises spawning, fractional velocities,
/// mid-match velocity changes, and slot reuse via despawn/respawn. Its final
/// state hash is pinned in the determinism test, so any accidental change to the
/// sim, math, or hashing surfaces immediately.
pub fn demo_replay() -> Replay {
    let mut r = Replay::new(0x00C0_FFEE_D00D_5EED);

    // Tick 0: spawn four units with whole and fractional per-tick velocities.
    r.record(vec![
        Command::Spawn {
            owner: 0,
            pos: v(0, 0, 0),
            vel: vr(1, 4, 0, 1, 0, 1),
        },
        Command::Spawn {
            owner: 0,
            pos: v(10, 0, 0),
            vel: vr(-1, 3, 1, 8, 0, 1),
        },
        Command::Spawn {
            owner: 1,
            pos: v(0, 10, 0),
            vel: vr(0, 1, -1, 5, 2, 7),
        },
        Command::Spawn {
            owner: 1,
            pos: v(-5, -5, 2),
            vel: vr(1, 2, 1, 2, -1, 2),
        },
    ]);

    // Ticks 1..8: free integration.
    for _ in 0..8 {
        r.record(vec![]);
    }

    // Tick 9: redirect unit 2.
    r.record(vec![Command::SetVelocity {
        entity_index: 2,
        vel: vr(-3, 7, 0, 1, 0, 1),
    }]);

    // Ticks 10..15: integrate.
    for _ in 0..6 {
        r.record(vec![]);
    }

    // Tick 16: despawn unit 1 (slot 1 goes on the free list).
    r.record(vec![Command::Despawn { entity_index: 1 }]);

    // Tick 17: spawn a new unit — reuses slot 1 with a bumped generation.
    r.record(vec![Command::Spawn {
        owner: 0,
        pos: v(3, 3, 3),
        vel: vr(0, 1, 0, 1, 1, 10),
    }]);

    // Ticks 18..40: integrate to the end.
    for _ in 0..23 {
        r.record(vec![]);
    }

    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_runs_to_expected_length() {
        let r = demo_replay();
        let w = run_replay(&r);
        assert_eq!(w.tick(), r.len() as u64);
        // Four spawned, one despawned, one respawned => five live? No: 4 - 1 + 1.
        assert_eq!(w.alive_count(), 4);
    }
}
