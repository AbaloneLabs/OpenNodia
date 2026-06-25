//! Round (block height) type.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A blockchain round (block height). Algorand produces a block roughly
/// every 3.3 seconds.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
pub struct Round(pub u64);

impl Round {
    /// Raw u64 value.
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Approximate number of seconds until `other`, given ~3.3s block time.
    pub fn secs_until(self, other: Self) -> u64 {
        if other.0 <= self.0 {
            0
        } else {
            (other.0 - self.0) * 33 / 10
        }
    }
}

impl fmt::Debug for Round {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Round({})", self.0)
    }
}

impl fmt::Display for Round {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::ops::Add<u64> for Round {
    type Output = Self;
    fn add(self, rhs: u64) -> Self {
        Self(self.0 + rhs)
    }
}

impl std::ops::Sub<u64> for Round {
    type Output = Self;
    fn sub(self, rhs: u64) -> Self {
        Self(self.0.saturating_sub(rhs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic() {
        assert_eq!((Round(100) + 50).as_u64(), 150);
        assert_eq!((Round(100) - 30).as_u64(), 70);
        assert_eq!((Round(10) - 100).as_u64(), 0); // saturating
    }

    #[test]
    fn secs_until() {
        // ~3.3s per round: 100 rounds ≈ 330s
        assert_eq!(Round(0).secs_until(Round(100)), 330);
        assert_eq!(Round(100).secs_until(Round(0)), 0);
    }
}
