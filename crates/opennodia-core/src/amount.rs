//! Amount type representing microAlgos (1 ALGO = 1,000,000 microAlgos).

use std::fmt;
use std::ops::{Add, Sub};

use serde::{Deserialize, Serialize};

/// Amount in microAlgos. 1 ALGO = 1_000_000 microAlgos.
/// The minimum transaction fee on Algorand is 1,000 microAlgos (0.001 ALGO).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
pub struct MicroAlgo(pub u64);

impl MicroAlgo {
    /// One microAlgo.
    pub const ONE: Self = Self(1);

    /// One ALGO in microAlgos.
    pub const ONE_ALGO: Self = Self(1_000_000);

    /// The minimum transaction fee (1,000 microAlgos = 0.001 ALGO).
    pub const MIN_FEE: Self = Self(1_000);

    /// The per-asset minimum balance requirement (0.1 ALGO = 100,000 microAlgos).
    pub const PER_ASSET_MIN_BALANCE: Self = Self(100_000);

    /// The base account minimum balance (0.1 ALGO).
    pub const BASE_MIN_BALANCE: Self = Self(100_000);

    /// Create from whole ALGO units.
    pub const fn from_algos(algos: u64) -> Self {
        Self(algos * 1_000_000)
    }

    /// Raw microAlgo value.
    pub const fn as_micro(self) -> u64 {
        self.0
    }

    /// Convert to f64 ALGO units for display.
    pub fn as_algos_f64(self) -> f64 {
        self.0 as f64 / 1_000_000.0
    }

    /// Format as a human-readable ALGO string with exact precision.
    ///
    /// Always shows exactly 6 decimal places, e.g. `201000` → `"0.201000 ALGO"`.
    /// Integer arithmetic only (no f64 rounding drift).
    pub fn fmt_algo(self) -> String {
        let whole = self.0 / 1_000_000;
        let frac = self.0 % 1_000_000;
        format!("{whole}.{frac:06} ALGO")
    }

    /// Saturating subtraction (never underflows).
    pub fn saturating_sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }
}

impl fmt::Debug for MicroAlgo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} microAlgo", self.0)
    }
}

impl fmt::Display for MicroAlgo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Add for MicroAlgo {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl Sub for MicroAlgo {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_algos_conversion() {
        assert_eq!(MicroAlgo::from_algos(1), MicroAlgo::ONE_ALGO);
        assert_eq!(MicroAlgo::from_algos(1).as_micro(), 1_000_000);
    }

    #[test]
    fn min_fee() {
        assert_eq!(MicroAlgo::MIN_FEE.as_micro(), 1_000);
    }

    #[test]
    fn per_asset_min_balance() {
        assert_eq!(MicroAlgo::PER_ASSET_MIN_BALANCE.as_micro(), 100_000);
    }

    #[test]
    fn arithmetic() {
        let a = MicroAlgo::from_algos(2);
        let b = MicroAlgo::from_algos(1);
        assert_eq!((a + b).as_micro(), 3_000_000);
        assert_eq!((a - b).as_micro(), 1_000_000);
    }

    #[test]
    fn saturating_sub_no_underflow() {
        let a = MicroAlgo(100);
        let b = MicroAlgo(200);
        assert_eq!(a.saturating_sub(b), MicroAlgo(0));
    }

    #[test]
    fn float_display() {
        let a = MicroAlgo::from_algos(1);
        assert!((a.as_algos_f64() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn fmt_algo_exact_precision() {
        // Sub-ALGO amounts show leading zeros.
        assert_eq!(MicroAlgo(201_000).fmt_algo(), "0.201000 ALGO");
        assert_eq!(MicroAlgo(1_000).fmt_algo(), "0.001000 ALGO");
        assert_eq!(MicroAlgo(100_000).fmt_algo(), "0.100000 ALGO");
        // Whole ALGO amounts.
        assert_eq!(MicroAlgo::from_algos(1).fmt_algo(), "1.000000 ALGO");
        assert_eq!(MicroAlgo::from_algos(0).fmt_algo(), "0.000000 ALGO");
        // Large amount with fractional part.
        assert_eq!(MicroAlgo(1_500_500).fmt_algo(), "1.500500 ALGO");
    }
}
