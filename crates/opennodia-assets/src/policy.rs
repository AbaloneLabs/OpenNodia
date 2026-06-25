//! Asset policy grade classification.
//!
//! The policy model keeps freely transferable assets separate from assets that
//! have freeze, clawback, or default-frozen controls.
//!
//! | Grade | Condition | DEX support |
//! |-------|-----------|-------------|
//! | Open | no freeze, no clawback, default-frozen=false | full |
//! | Bridged | bridged asset, source shown | conditional |
//! | Regulated | freeze/clawback present or default-frozen=true | disabled by default |

use serde::{Deserialize, Serialize};

/// Classification of an ASA based on its configuration parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetPolicyGrade {
    /// Freely transferable ASA: no freeze, no clawback, not default-frozen.
    Open,
    /// Bridged from another chain (source to be displayed).
    Bridged,
    /// Regulated asset: freeze/clawback present or default-frozen.
    /// DEX trading disabled by default.
    Regulated,
}

impl AssetPolicyGrade {
    /// Classify an asset from its raw configuration flags.
    pub fn classify(has_freeze: bool, has_clawback: bool, default_frozen: bool) -> Self {
        if default_frozen || has_freeze || has_clawback {
            Self::Regulated
        } else {
            Self::Open
        }
    }

    /// Whether this grade is tradeable on the DEX by default.
    pub const fn is_tradeable_by_default(self) -> bool {
        matches!(self, Self::Open)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_grade() {
        assert_eq!(
            AssetPolicyGrade::classify(false, false, false),
            AssetPolicyGrade::Open
        );
        assert!(AssetPolicyGrade::Open.is_tradeable_by_default());
    }

    #[test]
    fn regulated_grade() {
        assert_eq!(
            AssetPolicyGrade::classify(true, false, false),
            AssetPolicyGrade::Regulated
        );
        assert_eq!(
            AssetPolicyGrade::classify(false, true, false),
            AssetPolicyGrade::Regulated
        );
        assert_eq!(
            AssetPolicyGrade::classify(false, false, true),
            AssetPolicyGrade::Regulated
        );
        assert!(!AssetPolicyGrade::Regulated.is_tradeable_by_default());
    }
}
