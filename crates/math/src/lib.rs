//! Deterministic fixed-point math for the simulation core.
//!
//! The simulation must be bit-identical on every CPU, OS, and run (see
//! `docs/architecture/01-determinism.md`), which rules out floating point -
//! its rounding, transcendentals, and fused-multiply-add differ across
//! hardware. [`Fx`] is an integer-backed fixed-point scalar whose every
//! operation reduces to integer arithmetic and is therefore identical
//! everywhere. [`Vec2`]/[`Vec3`] build on it.
//!
//! This is a small, dependency-free implementation so the most safety-critical
//! crate in the engine has zero external surface and full control over
//! overflow/rounding semantics. It can later be swapped to back onto the
//! `fixed` crate without changing call sites.

#![forbid(unsafe_code)]

mod scalar;
mod vec;

pub use scalar::{Fx, FRAC_BITS};
pub use vec::{Vec2, Vec3};
