//! Deterministic pseudo-random number generator.
//!
//! Lives *inside* the world state and is part of the hashed state
//! (`docs/architecture/01-determinism.md`). The algorithm is **pinned**
//! (SplitMix64) and must never change without bumping the sim version - every
//! peer must draw the same sequence in the same order. Never use `rand`'s
//! thread RNG in the sim: it is OS-seeded and non-portable.

/// SplitMix64 - a small, fast, fully deterministic PRNG.
#[derive(Clone, Debug)]
pub struct DetRng {
    state: u64,
}

impl DetRng {
    /// Seed from the shared match seed (identical on every peer).
    #[inline]
    pub fn new(seed: u64) -> Self {
        DetRng { state: seed }
    }

    /// Advance and return the next 64 bits.
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform-ish integer in `0..n` (`n > 0`).
    #[inline]
    pub fn range_u32(&mut self, n: u32) -> u32 {
        debug_assert!(n > 0);
        (self.next_u64() % n as u64) as u32
    }

    /// The raw internal state, for hashing/serialization.
    #[inline]
    pub fn raw(&self) -> u64 {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_sequence() {
        let mut a = DetRng::new(12345);
        let mut b = DetRng::new(12345);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = DetRng::new(1);
        let mut b = DetRng::new(2);
        assert_ne!(a.next_u64(), b.next_u64());
    }
}
