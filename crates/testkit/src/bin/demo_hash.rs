//! Prints the demo replay's final state hash.
//!
//! Run on any platform - the value must be identical everywhere. Used to (re)pin
//! the golden constant in `tests/determinism.rs`:
//!     cargo run -p testkit --bin demo_hash

fn main() {
    let replay = testkit::demo_replay();
    let world = testkit::run_replay(&replay);
    println!("ticks       = {}", replay.len());
    println!("alive_count = {}", world.alive_count());
    println!("state_hash  = {:#018x}", world.state_hash());
}
