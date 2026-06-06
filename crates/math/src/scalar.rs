//! [`Fx`]: a deterministic 64-bit fixed-point scalar.

use core::fmt;
use core::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// Number of fractional bits. With a signed 64-bit backing this is the `I40F24`
/// layout recommended in `docs/architecture/01-determinism.md`: ~7 decimal
/// digits of fractional precision and an integer range (±2^39) large enough for
/// big maps without overflow.
pub const FRAC_BITS: u32 = 24;

const ONE_RAW: i64 = 1 << FRAC_BITS;
const FRAC_MASK: u128 = (1u128 << FRAC_BITS) - 1;

/// A deterministic fixed-point number, backed by a raw `i64` equal to the value
/// times `2^FRAC_BITS`.
///
/// Every operation is integer arithmetic, so results are identical on all
/// platforms. `add`/`sub`/`neg` saturate; `mul`/`div` use an `i128` intermediate
/// and saturate on the way back. Multiplication truncates toward negative
/// infinity (arithmetic shift) and division toward zero — both fully
/// deterministic.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Fx(i64);

#[inline]
const fn sat_i128_to_i64(v: i128) -> i64 {
    if v > i64::MAX as i128 {
        i64::MAX
    } else if v < i64::MIN as i128 {
        i64::MIN
    } else {
        v as i64
    }
}

impl Fx {
    pub const ZERO: Fx = Fx(0);
    pub const ONE: Fx = Fx(ONE_RAW);
    pub const MIN: Fx = Fx(i64::MIN);
    pub const MAX: Fx = Fx(i64::MAX);

    /// Construct from the raw backing integer (value × `2^FRAC_BITS`).
    #[inline]
    pub const fn from_raw(raw: i64) -> Self {
        Fx(raw)
    }

    /// The raw backing integer. For serialization, hashing, and tests only.
    #[inline]
    pub const fn to_raw(self) -> i64 {
        self.0
    }

    /// Construct from a whole integer.
    #[inline]
    pub const fn from_int(i: i32) -> Self {
        Fx((i as i64) << FRAC_BITS)
    }

    /// Construct the exact ratio `num / den` (e.g. a fixed timestep). Returns
    /// [`Fx::ZERO`] if `den == 0` (guarded so a bad constant can't panic
    /// non-deterministically).
    #[inline]
    pub const fn from_ratio(num: i64, den: i64) -> Self {
        if den == 0 {
            return Fx::ZERO;
        }
        Fx(sat_i128_to_i64(((num as i128) << FRAC_BITS) / den as i128))
    }

    /// Largest integer `<= self` (floor).
    #[inline]
    pub const fn floor_int(self) -> i64 {
        self.0 >> FRAC_BITS
    }

    /// Absolute value (saturating).
    #[inline]
    pub const fn abs(self) -> Fx {
        Fx(self.0.saturating_abs())
    }

    /// Deterministic fixed-point square root (returns 0 for negative input).
    /// Uses integer Newton's method on a widened value, so it is bit-identical
    /// on every platform.
    pub fn sqrt(self) -> Fx {
        if self.0 <= 0 {
            return Fx::ZERO;
        }
        // result = isqrt(raw << FRAC_BITS), computed in u128 to avoid overflow.
        let n: u128 = (self.0 as u128) << FRAC_BITS;
        let mut x = n;
        let mut y = (x + 1) >> 1;
        while y < x {
            x = y;
            y = (x + n / x) >> 1;
        }
        Fx(x as i64)
    }

    #[inline]
    pub fn min(self, other: Fx) -> Fx {
        if self.0 <= other.0 {
            self
        } else {
            other
        }
    }

    #[inline]
    pub fn max(self, other: Fx) -> Fx {
        if self.0 >= other.0 {
            self
        } else {
            other
        }
    }
}

impl Add for Fx {
    type Output = Fx;
    #[inline]
    fn add(self, rhs: Fx) -> Fx {
        Fx(self.0.saturating_add(rhs.0))
    }
}

impl Sub for Fx {
    type Output = Fx;
    #[inline]
    fn sub(self, rhs: Fx) -> Fx {
        Fx(self.0.saturating_sub(rhs.0))
    }
}

impl Neg for Fx {
    type Output = Fx;
    #[inline]
    fn neg(self) -> Fx {
        Fx(self.0.saturating_neg())
    }
}

impl Mul for Fx {
    type Output = Fx;
    #[inline]
    fn mul(self, rhs: Fx) -> Fx {
        let prod = (self.0 as i128) * (rhs.0 as i128);
        Fx(sat_i128_to_i64(prod >> FRAC_BITS))
    }
}

impl Div for Fx {
    type Output = Fx;
    #[inline]
    fn div(self, rhs: Fx) -> Fx {
        debug_assert!(rhs.0 != 0, "Fx division by zero");
        if rhs.0 == 0 {
            return Fx::ZERO;
        }
        let num = (self.0 as i128) << FRAC_BITS;
        Fx(sat_i128_to_i64(num / rhs.0 as i128))
    }
}

impl AddAssign for Fx {
    #[inline]
    fn add_assign(&mut self, rhs: Fx) {
        *self = *self + rhs;
    }
}

impl SubAssign for Fx {
    #[inline]
    fn sub_assign(&mut self, rhs: Fx) {
        *self = *self - rhs;
    }
}

impl fmt::Debug for Fx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Decimal rendering via integer math only (no floats in this crate).
        let neg = self.0 < 0;
        let mag = (self.0 as i128).unsigned_abs();
        let int_part = mag >> FRAC_BITS;
        let micros = ((mag & FRAC_MASK) * 1_000_000) >> FRAC_BITS;
        write!(
            f,
            "{}{}.{:06}",
            if neg { "-" } else { "" },
            int_part,
            micros
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_construction_and_ops() {
        assert_eq!(Fx::from_int(2) + Fx::from_int(3), Fx::from_int(5));
        assert_eq!(Fx::from_int(5) - Fx::from_int(8), Fx::from_int(-3));
        assert_eq!(Fx::from_int(2) * Fx::from_int(3), Fx::from_int(6));
        assert_eq!(Fx::from_int(6) / Fx::from_int(3), Fx::from_int(2));
        assert_eq!(-Fx::from_int(4), Fx::from_int(-4));
    }

    #[test]
    fn unit_and_ratio() {
        assert_eq!(Fx::ONE * Fx::ONE, Fx::ONE);
        assert_eq!(Fx::from_ratio(1, 2) + Fx::from_ratio(1, 2), Fx::ONE);
        assert_eq!(Fx::from_ratio(1, 4) * Fx::from_int(4), Fx::ONE);
        assert_eq!(Fx::from_ratio(7, 0), Fx::ZERO); // guarded div-by-zero
    }

    #[test]
    fn multiplication_is_commutative_and_deterministic() {
        let a = Fx::from_ratio(355, 113);
        let b = Fx::from_ratio(-22, 7);
        assert_eq!(a * b, b * a);
        // Re-running identical ops yields identical raw bits.
        assert_eq!((a * b).to_raw(), (a * b).to_raw());
    }

    #[test]
    fn saturation_does_not_panic() {
        assert_eq!(Fx::MAX + Fx::ONE, Fx::MAX);
        assert_eq!(Fx::MIN - Fx::ONE, Fx::MIN);
        assert_eq!(-Fx::MIN, Fx::MAX);
    }

    #[test]
    fn ordering_matches_value() {
        assert!(Fx::from_int(-1) < Fx::ZERO);
        assert!(Fx::from_ratio(1, 2) < Fx::ONE);
        assert_eq!(Fx::from_int(3).max(Fx::from_int(5)), Fx::from_int(5));
    }
}
