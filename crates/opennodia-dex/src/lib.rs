//! Local orderbook for OpenNodia DEX: SQLite cache + on-chain event tracking.
//!
//! This crate provides:
//! - [`db`]: SQLite schema and order/trade persistence.
//! - [`types`]: order entries, pairs, snapshots, trades.
//! - [`orderbook`]: aggregate active orders into bid/ask price levels.
//! - [`events`]: detect fills/cancels/expiries by polling escrow accounts.
//! - [`cache`]: delta-sync bookkeeping.

pub mod cache;
pub mod db;
pub mod events;
pub mod orderbook;
pub mod types;

pub use cache::{init_sync_state, sync_from_round, SyncState};
pub use db::DexDb;
pub use events::{
    classify_confirmed_group, classify_escrow_event, poll_order, resolve_closed_order,
    sweep_active_orders, EscrowEvent, SweepResult,
};
pub use orderbook::{
    build_snapshot_from_orders, get_orderbook, get_recent_trades, get_trades_for_account,
    sweep_expired,
};
pub use types::{
    CommunityMarket, EntryStatus, OrderBookSnapshot, OrderEntry, Pair, PairStat, PriceLevel, Trade,
};

use opennodia_swap::matching::BookOrder;
use opennodia_swap::matching::OrderbookSource;

/// Adapter implementing [`OrderbookSource`] over the SQLite-backed orderbook.
pub struct DexOrderbookSource<'a> {
    db: &'a DexDb,
}

impl<'a> DexOrderbookSource<'a> {
    pub fn new(db: &'a DexDb) -> Self {
        Self { db }
    }
}

impl<'a> OrderbookSource for DexOrderbookSource<'a> {
    fn get_opposite_orders(
        &self,
        side: opennodia_swap::OrderSide,
        sell_asset: u64,
        buy_asset: u64,
        max_price: Option<u64>,
    ) -> Vec<BookOrder> {
        let pair = types::Pair::new(sell_asset, buy_asset);
        let orders = match self.db.get_active_orders_for_pair(pair) {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!(error = %e, "orderbook query failed");
                return Vec::new();
            }
        };

        let opposite_side = match side {
            opennodia_swap::OrderSide::Buy => opennodia_swap::OrderSide::Sell,
            opennodia_swap::OrderSide::Sell => opennodia_swap::OrderSide::Buy,
        };

        orders
            .into_iter()
            .filter(|o| {
                // Cross the trading pair: an incoming order that sells A for B
                // matches an opposite order that sells B for A.
                let matches = if side == opennodia_swap::OrderSide::Buy {
                    // Incoming buy: I pay sell_asset to receive buy_asset.
                    // Opposite sell: they sell buy_asset for sell_asset.
                    o.side == opposite_side
                        && o.sell_asset == buy_asset
                        && o.buy_asset == sell_asset
                } else {
                    // Incoming sell: I sell sell_asset for buy_asset.
                    // Opposite buy: they sell buy_asset for sell_asset.
                    o.side == opposite_side
                        && o.sell_asset == buy_asset
                        && o.buy_asset == sell_asset
                };
                if !matches {
                    return false;
                }
                // Price filter.
                match max_price {
                    Some(mp) => o.price <= mp,
                    None => true,
                }
            })
            .map(|o| BookOrder {
                escrow_address: o.escrow_addr,
                owner: o.owner,
                side: o.side,
                sell_asset: o.sell_asset,
                sell_amount: o.sell_amount,
                buy_asset: o.buy_asset,
                buy_amount: o.buy_amount,
                available_amount: o.remaining(),
                program: o.program,
                params: o.params,
            })
            .collect()
    }
}

// Re-export commonly used matching types for callers.
pub use opennodia_swap::matching::{FillCandidate as _FillCandidate, MatchResult, RoutingDecision};

#[cfg(test)]
mod tests {
    use super::*;
    use opennodia_core::Address;
    use opennodia_swap::{EscrowParams, OrderSide};

    fn make_entry(
        side: OrderSide,
        sell_asset: u64,
        sell_amount: u64,
        buy_asset: u64,
        buy_amount: u64,
        escrow_byte: u8,
    ) -> OrderEntry {
        let owner = Address::from_bytes([1u8; 32]);
        let price = if sell_amount == 0 {
            0
        } else {
            ((buy_amount as u128) * 1_000_000 / sell_amount as u128) as u64
        };
        let program = vec![escrow_byte, 0x01, 0x02];
        OrderEntry {
            escrow_addr: opennodia_swap::escrow_address(&program),
            side,
            sell_asset,
            sell_amount,
            buy_asset,
            buy_amount,
            price,
            owner,
            created_round: opennodia_core::Round(1000),
            expire_round: opennodia_core::Round(100_000),
            status: EntryStatus::Active,
            filled_amount: 0,
            split_index: 0,
            parent_id: None,
            program,
            params: EscrowParams::new(
                owner,
                sell_asset,
                sell_amount,
                buy_asset,
                buy_amount,
                100_000,
            ),
        }
    }

    #[test]
    fn dex_orderbook_source_returns_opposite_orders() {
        let db = DexDb::open_memory().unwrap();
        // Sell order: 1000 ASA for 2M microAlgo.
        // price_micro = 2_000_000 * 1e6 / 1000 = 2_000_000_000.
        db.register_order(&make_entry(
            OrderSide::Sell,
            12345,
            1000,
            0,
            2_000_000,
            0xAA,
        ))
        .unwrap();

        let source = DexOrderbookSource::new(&db);
        // Incoming buy of ASA with ALGO → should match the sell order.
        // max_price in micro-ratio scale must be >= the ask's price_micro.
        let orders = source.get_opposite_orders(OrderSide::Buy, 0, 12345, Some(3_000_000_000));
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].sell_amount, 1000);
    }

    #[test]
    fn dex_orderbook_source_filters_by_price() {
        let db = DexDb::open_memory().unwrap();
        // Expensive sell: 100 ASA for 10M microAlgo.
        // price_micro = 10_000_000 * 1e6 / 100 = 100_000_000.
        db.register_order(&make_entry(
            OrderSide::Sell,
            12345,
            100,
            0,
            10_000_000,
            0xBB,
        ))
        .unwrap();

        let source = DexOrderbookSource::new(&db);
        // Buyer willing to pay only up to price_micro 10_000 → ask (100_000_000)
        // is too expensive and is filtered out.
        let orders = source.get_opposite_orders(OrderSide::Buy, 0, 12345, Some(10_000));
        assert!(orders.is_empty());
    }
}
