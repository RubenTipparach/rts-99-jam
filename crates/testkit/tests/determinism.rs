//! The keystone test of the whole engine (`docs/architecture/01-determinism.md`).
//!
//! A fixed command log, run through the integer-only simulation, must reduce to
//! one specific state hash - the *same* hash on every OS, CPU, and run. CI runs
//! this on every push.

use testkit::{demo_replay, final_hash};

/// Running the same replay twice in one process yields the same hash.
#[test]
fn reproducible_within_process() {
    assert_eq!(final_hash(&demo_replay()), final_hash(&demo_replay()));
}

/// The pinned cross-platform result. Because the sim uses only integer/
/// fixed-point math, this constant must hold on Linux, Windows, macOS, and
/// `wasm32` alike.
///
/// If this fails: either determinism regressed (a real bug - a stray float, an
/// unordered iteration, a non-pinned RNG) or the sim/math/hash changed on
/// purpose. In the latter case, recompute with
/// `cargo run -p testkit --bin demo_hash` and update this value.
#[test]
fn matches_pinned_cross_platform_hash() {
    const GOLDEN: u64 = 0x839f_168a_09a1_ed09;
    assert_eq!(final_hash(&demo_replay()), GOLDEN);
}
