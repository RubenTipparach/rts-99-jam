//! Headless simulation harness.
//!
//! No window, no renderer — just drive [`sim::World`] from a [`Replay`] and read
//! the resulting state hash. Powers the determinism CI and (later) bot-vs-bot.
//! See `docs/architecture/10-roadmap-testing.md`.

#![forbid(unsafe_code)]

use math::Fx;
use protocol::{BuildingKind, Command, UnitKind};
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

/// Run a replay and return the final state hash — identical on every platform.
pub fn final_hash(replay: &Replay) -> u64 {
    run_replay(replay).state_hash()
}

#[inline]
fn fx(i: i32) -> Fx {
    Fx::from_int(i)
}

/// A fixed reference battle: two barracks per side that produce infantry, plus a
/// few starting units that march and fight. Exercises spawning, production,
/// movement, and combat. Its final state hash is pinned in the determinism test.
pub fn demo_replay() -> Replay {
    let mut r = Replay::new(0x00C0_FFEE_D00D_5EED);

    let mut setup = vec![
        Command::SpawnBuilding {
            owner: 0,
            kind: BuildingKind::Barracks,
            x: fx(0),
            y: fx(-22),
        },
        Command::SpawnBuilding {
            owner: 1,
            kind: BuildingKind::Barracks,
            x: fx(-10),
            y: fx(22),
        },
        Command::SpawnBuilding {
            owner: 1,
            kind: BuildingKind::Barracks,
            x: fx(10),
            y: fx(22),
        },
    ];
    for k in 0..4 {
        setup.push(Command::SpawnUnit {
            owner: 0,
            kind: UnitKind::Infantry,
            x: fx(-3 + 2 * k),
            y: fx(-18),
        });
        setup.push(Command::SpawnUnit {
            owner: 1,
            kind: UnitKind::Infantry,
            x: fx(-3 + 2 * k),
            y: fx(18),
        });
    }
    r.record(setup);

    // Send the player's starting infantry north to attack.
    r.record(vec![
        Command::AttackMove {
            unit: 3,
            x: fx(0),
            y: fx(22),
        },
        Command::AttackMove {
            unit: 4,
            x: fx(0),
            y: fx(22),
        },
        Command::AttackMove {
            unit: 5,
            x: fx(0),
            y: fx(22),
        },
        Command::AttackMove {
            unit: 6,
            x: fx(0),
            y: fx(22),
        },
    ]);

    // Queue a few units at the player's barracks (building index 0) so the
    // manual production path is exercised by the determinism test.
    r.record(vec![
        Command::Train { building: 0 },
        Command::Train { building: 0 },
        Command::Train { building: 0 },
    ]);

    for _ in 0..300 {
        r.record(vec![]);
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_runs_and_does_something() {
        let w = run_replay(&demo_replay());
        assert_eq!(w.tick(), demo_replay().len() as u64);
        assert!(w.alive_count() > 0);
    }
}
