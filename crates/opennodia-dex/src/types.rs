//! Orderbook types: order entries, pairs, snapshots, and trades.
//!
use opennodia_core::{Address, Round};
use opennodia_swap::{EscrowKind, EscrowParams, OrderSide, OrderStatus};
use serde::{Deserialize, Serialize};

/// Order status as stored in the orderbook DB.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryStatus {
    Active,
    Filled,
    Cancelled,
    Expired,
    ClosedUnresolved,
}

impl EntryStatus {
    /// Whether this order is still fillable.
    pub fn is_active(self) -> bool {
        matches!(self, EntryStatus::Active)
    }

    /// String form for SQLite storage.
    pub fn as_str(self) -> &'static str {
        match self {
            EntryStatus::Active => "active",
            EntryStatus::Filled => "filled",
            EntryStatus::Cancelled => "cancelled",
            EntryStatus::Expired => "expired",
            EntryStatus::ClosedUnresolved => "closed_unresolved",
        }
    }

    /// Parse a validated SQLite value.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "active" => Some(EntryStatus::Active),
            "filled" => Some(EntryStatus::Filled),
            "cancelled" => Some(EntryStatus::Cancelled),
            "expired" => Some(EntryStatus::Expired),
            "closed_unresolved" => Some(EntryStatus::ClosedUnresolved),
            _ => None,
        }
    }
}

impl From<OrderStatus> for EntryStatus {
    fn from(s: OrderStatus) -> Self {
        match s {
            OrderStatus::Open => EntryStatus::Active,
            OrderStatus::Filled => EntryStatus::Filled,
            OrderStatus::Cancelled => EntryStatus::Cancelled,
            OrderStatus::Expired => EntryStatus::Expired,
        }
    }
}

/// A canonical trading pair (lower asset ID first).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Pair {
    pub asset_a: u64,
    pub asset_b: u64,
}

impl Pair {
    /// Create a canonical pair from two asset IDs (0 = ALGO).
    pub fn new(a: u64, b: u64) -> Self {
        if a <= b {
            Self {
                asset_a: a,
                asset_b: b,
            }
        } else {
            Self {
                asset_a: b,
                asset_b: a,
            }
        }
    }

    /// Whether this pair contains the given asset.
    pub fn contains(self, asset: u64) -> bool {
        self.asset_a == asset || self.asset_b == asset
    }

    /// The "other" asset in the pair.
    pub fn other(self, asset: u64) -> u64 {
        if self.asset_a == asset {
            self.asset_b
        } else {
            self.asset_a
        }
    }

    /// String form for display (e.g. "ALGO/12345").
    pub fn display(self) -> String {
        let a = if self.asset_a == 0 {
            "ALGO"
        } else {
            &self.asset_a.to_string()
        };
        let b = if self.asset_b == 0 {
            "ALGO"
        } else {
            &self.asset_b.to_string()
        };
        format!("{a}/{b}")
    }
}

/// Operator-authenticated community market metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommunityMarket {
    /// Stable local market id.
    pub id: String,
    /// ASA operator address that signed the registration payload.
    pub operator: Address,
    /// Display name for the community market.
    pub name: String,
    /// Operator-provided market description.
    pub description: String,
    /// Optional logo URL. Empty when absent.
    pub logo_url: String,
    /// Official ASA ids claimed by this market. ALGO is represented only in pairs.
    pub asset_ids: Vec<u64>,
    /// Official trading pairs for this market.
    pub pairs: Vec<Pair>,
    /// Optional operator migration or deprecation notice.
    pub migration_notice: Option<String>,
    /// Reserved hook for future signed announcement channels.
    pub announcement_channel: Option<String>,
    /// Base64 signature over the canonical market payload.
    pub signature: String,
    /// Operator-provided update timestamp in Unix seconds.
    pub updated_at: u64,
}

/// A stored orderbook order entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderEntry {
    /// Escrow address (hex string in DB).
    pub escrow_addr: Address,
    /// Side of the order.
    pub side: OrderSide,
    /// ASA being sold (0 = ALGO).
    pub sell_asset: u64,
    /// Raw units of the sell asset.
    pub sell_amount: u64,
    /// Asset being bought (0 = ALGO).
    pub buy_asset: u64,
    /// Raw units of the buy asset.
    pub buy_amount: u64,
    /// Price (buy per sell unit) as a micro-ratio (×1e6).
    pub price: u64,
    /// Owner address.
    pub owner: Address,
    /// Round when the order was created.
    pub created_round: Round,
    /// Round after which the order is invalid for fills.
    pub expire_round: Round,
    /// Current status.
    pub status: EntryStatus,
    /// How much has been filled (raw units). v1 escrows are atomic whole-fills,
    /// so this is 0 or sell_amount.
    pub filled_amount: u64,
    /// Split index (0-based) for split orders.
    pub split_index: u32,
    /// Parent order id (escrow addr hex) for split children.
    pub parent_id: Option<String>,
    /// Compiled TEAL program (with embedded params).
    #[serde(default)]
    pub program: Vec<u8>,
    /// Escrow params for re-derivation.
    pub params: EscrowParams,
}

impl OrderEntry {
    /// Remaining unfilled amount.
    pub fn remaining(&self) -> u64 {
        self.sell_amount.saturating_sub(self.filled_amount)
    }

    /// Whether this entry is for the given pair (in either direction).
    pub fn matches_pair(&self, pair: Pair) -> bool {
        let entry_pair = Pair::new(self.sell_asset, self.buy_asset);
        entry_pair == pair
    }

    /// The escrow kind (Sell or Buy).
    pub fn escrow_kind(&self) -> EscrowKind {
        match self.side {
            OrderSide::Sell => EscrowKind::Sell,
            OrderSide::Buy => EscrowKind::Buy,
        }
    }

    /// Price normalized as lower-ID asset units per higher-ID asset unit.
    pub fn normalized_price(&self) -> Option<u64> {
        order_price(
            self.side,
            self.sell_asset,
            self.sell_amount,
            self.buy_asset,
            self.buy_amount,
        )
    }

    /// Amount of the UI base asset represented by this order.
    pub fn base_amount(&self) -> u64 {
        match self.side {
            OrderSide::Sell => self.sell_amount,
            OrderSide::Buy => self.buy_amount,
        }
    }
}

/// Price in quote units per base unit, matching the trading UI convention.
pub fn order_price(
    side: OrderSide,
    sell_asset: u64,
    sell_amount: u64,
    buy_asset: u64,
    buy_amount: u64,
) -> Option<u64> {
    if sell_asset == buy_asset || sell_amount == 0 || buy_amount == 0 {
        return None;
    }
    let (numerator, denominator) = match side {
        OrderSide::Sell => (buy_amount, sell_amount),
        OrderSide::Buy => (sell_amount, buy_amount),
    };
    Some(
        ((u128::from(numerator) * 1_000_000) / u128::from(denominator)).min(u128::from(u64::MAX))
            as u64,
    )
}

/// Return the reciprocal of a micro-ratio price.
///
/// A price of `20_000` represents `0.02`; its reciprocal is therefore
/// `50_000_000`, representing `50`. Zero has no reciprocal and returns zero.
pub fn invert_price(micro_price: u64) -> u64 {
    if micro_price == 0 {
        return 0;
    }
    ((1_000_000u128 * 1_000_000u128) / u128::from(micro_price)).min(u128::from(u64::MAX)) as u64
}

/// A snapshot of the orderbook for a pair at a point in time.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    pub pair: Pair,
    /// Bids (buy orders), sorted highest price first.
    pub bids: Vec<PriceLevel>,
    /// Asks (sell orders), sorted lowest price first.
    pub asks: Vec<PriceLevel>,
    /// Best bid - best ask (micro-ratio). 0 if one side is empty.
    pub spread: u64,
    /// Last trade price (micro-ratio), if any.
    pub last_price: Option<u64>,
    /// Round when this snapshot was assembled.
    pub last_update_round: Round,
}

/// An aggregated price level in the orderbook.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceLevel {
    /// Price (micro-ratio).
    pub price: u64,
    /// Total amount available at this price.
    pub amount: u64,
    /// Number of orders at this level.
    pub order_count: usize,
    /// Cumulative amount in the selected orderbook view's base asset up to and
    /// including this level.
    pub total: u64,
}

/// A recorded trade.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trade {
    /// Transaction id (hex).
    pub tx_id: String,
    /// The trading pair.
    pub pair: Pair,
    /// Side from the taker's perspective.
    pub side: OrderSide,
    /// Execution price (micro-ratio).
    pub price: u64,
    /// Base asset used by `price` and `amount`.
    ///
    /// Legacy rows created before pair-direction tracking have no value.
    #[serde(default)]
    pub base_asset: Option<u64>,
    /// Amount traded (raw units of `base_asset` for direction-aware rows).
    pub amount: u64,
    /// Buyer address.
    pub buyer: Address,
    /// Seller address.
    pub seller: Address,
    /// Round when the trade confirmed.
    pub round: Round,
    /// Unix timestamp (seconds).
    pub timestamp: u64,
}

/// Aggregated statistics for a trading pair, used to rank "popular" pairs.
///
/// `score` combines active order count, recent trade frequency, and recent
/// trade volume into a single number for sorting (higher = more popular).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairStat {
    /// The canonical pair.
    pub pair: Pair,
    /// Number of active orders for this pair.
    pub active_orders: u64,
    /// Number of trades in the recent window.
    pub recent_trade_count: u64,
    /// Raw units traded in the recent window (sell asset units, summed).
    pub recent_trade_volume: u64,
    /// Last execution price (micro-ratio), if any trade exists.
    pub last_price: Option<u64>,
    /// Composite popularity score for sorting.
    pub score: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_canonical_ordering() {
        let p = Pair::new(12345, 0);
        assert_eq!(p.asset_a, 0);
        assert_eq!(p.asset_b, 12345);
        let p2 = Pair::new(0, 12345);
        assert_eq!(p, p2);
    }

    #[test]
    fn pair_contains_and_other() {
        let p = Pair::new(0, 12345);
        assert!(p.contains(0));
        assert!(p.contains(12345));
        assert!(!p.contains(999));
        assert_eq!(p.other(0), 12345);
        assert_eq!(p.other(12345), 0);
    }

    #[test]
    fn entry_remaining() {
        let e = OrderEntry {
            escrow_addr: Address::zero(),
            side: OrderSide::Sell,
            sell_asset: 12345,
            sell_amount: 1000,
            buy_asset: 0,
            buy_amount: 2_000_000,
            price: 2_000_000,
            owner: Address::zero(),
            created_round: Round(1000),
            expire_round: Round(100_000),
            status: EntryStatus::Active,
            filled_amount: 300,
            split_index: 0,
            parent_id: None,
            program: Vec::new(),
            params: EscrowParams::new(Address::zero(), 12345, 1000, 0, 2_000_000, 100_000),
        };
        assert_eq!(e.remaining(), 700);
    }

    #[test]
    fn entry_status_active_check() {
        assert!(EntryStatus::Active.is_active());
        assert!(!EntryStatus::Filled.is_active());
        assert!(!EntryStatus::Cancelled.is_active());
        assert!(!EntryStatus::Expired.is_active());
        assert!(!EntryStatus::ClosedUnresolved.is_active());
    }

    #[test]
    fn pair_display() {
        assert_eq!(Pair::new(0, 12345).display(), "ALGO/12345");
        assert_eq!(Pair::new(100, 200).display(), "100/200");
    }
}
