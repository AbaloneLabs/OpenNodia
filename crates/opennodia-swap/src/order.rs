//! Atomic swap order types.
//!
//! Escrow orders follow the non-custodial atomic swap model described in the
//! architecture and security documentation.

use opennodia_core::{Address, AssetId, Round};
use serde::{Deserialize, Serialize};

/// Which side of a trade an order represents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderSide {
    /// Selling an ASA for ALGO or another ASA.
    Sell,
    /// Buying an ASA with ALGO or another ASA.
    Buy,
}

impl OrderSide {
    /// String form for DB storage ("sell" / "buy").
    pub fn as_str(self) -> &'static str {
        match self {
            OrderSide::Sell => "sell",
            OrderSide::Buy => "buy",
        }
    }
}

/// The lifecycle status of an order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderStatus {
    /// Escrow funded, waiting for a fill.
    Open,
    /// Filled by a counterparty.
    Filled,
    /// Cancelled by the owner.
    Cancelled,
    /// Expired (past the expiry round).
    Expired,
}

impl OrderStatus {
    /// Whether this status represents an order that can still be filled.
    pub fn is_active(self) -> bool {
        matches!(self, OrderStatus::Open)
    }
}

/// An atomic swap order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Order {
    /// Unique order identifier (escrow address derived bytes).
    pub order_id: [u8; 32],
    /// Owner (seller for Sell, buyer for Buy).
    pub seller: Address,
    /// Escrow account address (LogicSig).
    pub escrow: Address,
    /// Side of the order.
    pub side: OrderSide,
    /// Asset being sold (ASA ID; 0 only valid as the *payment* side for Buy).
    pub sell_asset: AssetId,
    /// Amount to sell (raw units).
    pub sell_amount: u64,
    /// Asset being bought (ALGO = 0).
    pub buy_asset: AssetId,
    /// Amount to buy (raw units).
    pub buy_amount: u64,
    /// Round when the order was created.
    pub created_round: Round,
    /// Round after which the order is invalid for fills (enforced in-contract via `txn FirstValid`).
    pub expire_round: Round,
    /// Current status.
    pub status: OrderStatus,
    /// For split orders: which split index this is (0-based).
    #[serde(default)]
    pub split_index: u32,
    /// For split orders: the parent order id (if this is a child).
    #[serde(default)]
    pub parent_order_id: Option<[u8; 32]>,
}

impl Order {
    /// Whether the order has expired relative to the given current round.
    pub fn is_expired(&self, current_round: Round) -> bool {
        current_round > self.expire_round
    }

    /// Effective price (buy per sell unit) as a ratio, if sell_amount > 0.
    pub fn price_ratio(&self) -> Option<(u64, u64)> {
        if self.sell_amount == 0 {
            None
        } else {
            Some((self.buy_amount, self.sell_amount))
        }
    }

    /// Price per sell unit, expressed in buy-asset units (already micro for ALGO).
    ///
    /// For an ALGO-priced order this is microAlgo per sell unit. Computed as
    /// `buy_amount / sell_amount` (integer division).
    pub fn price_per_unit_micro(&self) -> u64 {
        if self.sell_amount == 0 {
            return 0;
        }
        self.buy_amount / self.sell_amount
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_order() -> Order {
        Order {
            order_id: [1u8; 32],
            seller: Address::zero(),
            escrow: Address::zero(),
            side: OrderSide::Sell,
            sell_asset: AssetId(12345),
            sell_amount: 100,
            buy_asset: AssetId::ALGO,
            buy_amount: 10_000_000,
            created_round: Round(1000),
            expire_round: Round(2000),
            status: OrderStatus::Open,
            split_index: 0,
            parent_order_id: None,
        }
    }

    #[test]
    fn expiry() {
        let o = sample_order();
        assert!(!o.is_expired(Round(1500)));
        assert!(o.is_expired(Round(2001)));
    }

    #[test]
    fn price_ratio() {
        let o = sample_order();
        assert_eq!(o.price_ratio(), Some((10_000_000, 100)));
    }

    #[test]
    fn price_per_unit() {
        let o = sample_order();
        // 10_000_000 microAlgo / 100 units = 100_000 microAlgo per unit.
        assert_eq!(o.price_per_unit_micro(), 100_000);
    }

    #[test]
    fn status_active() {
        assert!(OrderStatus::Open.is_active());
        assert!(!OrderStatus::Filled.is_active());
        assert!(!OrderStatus::Cancelled.is_active());
        assert!(!OrderStatus::Expired.is_active());
    }
}
