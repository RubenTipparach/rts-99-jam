//! Replays.
//!
//! Because the simulation is deterministic and consumes only commands, a whole
//! match is fully described by its **seed plus the per-tick command stream**.
//! Re-feeding that to a fresh [`sim::World`](../sim) reconstructs the match
//! byte-for-byte - so a replay is tiny, and it doubles as a determinism test
//! fixture and a bug-repro format. See `docs/architecture/03-networking-lockstep.md`.

#![forbid(unsafe_code)]

use protocol::Command;

/// A recorded match: the seed and the commands executed on each tick.
///
/// `turns[t]` holds the commands applied at tick `t`. An empty `Vec` is a valid,
/// meaningful turn ("no commands this tick").
#[derive(Clone, Debug, Default)]
pub struct Replay {
    pub seed: u64,
    pub turns: Vec<Vec<Command>>,
}

impl Replay {
    /// Start an empty recording for `seed`.
    pub fn new(seed: u64) -> Self {
        Replay {
            seed,
            turns: Vec::new(),
        }
    }

    /// Append the commands for the next tick.
    pub fn record(&mut self, commands: Vec<Command>) {
        self.turns.push(commands);
    }

    /// Number of recorded ticks.
    pub fn len(&self) -> usize {
        self.turns.len()
    }

    /// Whether any ticks have been recorded.
    pub fn is_empty(&self) -> bool {
        self.turns.is_empty()
    }
}
