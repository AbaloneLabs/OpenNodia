//! Algorand Standard Asset (ASA) identifier type.

use std::fmt;

use serde::{Deserialize, Serialize};

/// An Algorand Standard Asset (ASA) ID.
///
/// Asset ID 0 is a sentinel meaning "ALGO" (the native currency),
/// used in DEX pairs where one side is ALGO.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
pub struct AssetId(pub u64);

impl AssetId {
    /// Sentinel value representing native ALGO (not an ASA).
    pub const ALGO: Self = Self(0);

    /// Raw u64 value.
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Whether this represents native ALGO.
    pub const fn is_algo(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Debug for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_algo() {
            write!(f, "AssetId(ALGO)")
        } else {
            write!(f, "AssetId({})", self.0)
        }
    }
}

impl fmt::Display for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_algo() {
            write!(f, "ALGO")
        } else {
            write!(f, "{}", self.0)
        }
    }
}

impl From<u64> for AssetId {
    fn from(v: u64) -> Self {
        Self(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn algo_sentinel() {
        assert!(AssetId::ALGO.is_algo());
        assert!(!AssetId(12345).is_algo());
    }

    #[test]
    fn display() {
        assert_eq!(AssetId::ALGO.to_string(), "ALGO");
        assert_eq!(AssetId(12345).to_string(), "12345");
    }
}
