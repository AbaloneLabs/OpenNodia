//! Orderbook construction: aggregate active orders into bid/ask price levels.
//!
use crate::db::DexDb;
use crate::types::{invert_price, OrderBookSnapshot, OrderEntry, Pair, PriceLevel, Trade};
use opennodia_core::Round;

/// Build an orderbook snapshot for a pair from the database.
///
/// The last-traded price is looked up from the trades table so the UI can
/// show a meaningful "last price" instead of a perpetual dash.
pub fn get_orderbook(
    db: &DexDb,
    pair: Pair,
    view_base_asset: u64,
    current_round: Round,
) -> opennodia_core::Result<OrderBookSnapshot> {
    let orders = db.get_active_orders_for_pair(pair)?;
    let last_price = db
        .get_last_trade_price(pair, view_base_asset)
        .unwrap_or(None);
    Ok(build_snapshot_from_orders(
        pair,
        view_base_asset,
        &orders,
        current_round,
        last_price,
    ))
}

/// Build an orderbook snapshot from a list of active orders.
///
/// Orders are classified by the asset actually being sold, then prices and
/// amounts are normalized to the caller's selected base asset.
pub fn build_snapshot_from_orders(
    pair: Pair,
    view_base_asset: u64,
    orders: &[OrderEntry],
    current_round: Round,
    last_price: Option<u64>,
) -> OrderBookSnapshot {
    let mut asks_raw = Vec::new();
    let mut bids_raw = Vec::new();

    for o in orders {
        if !o.matches_pair(pair) {
            continue;
        }
        if let Some(order) = order_for_view(o, view_base_asset) {
            if o.sell_asset == view_base_asset {
                asks_raw.push(order);
            } else {
                bids_raw.push(order);
            }
        }
    }

    let asks = aggregate_levels(&asks_raw, true);
    let bids = aggregate_levels(&bids_raw, false);

    let spread = match (asks.first(), bids.first()) {
        (Some(a), Some(b)) if a.price >= b.price => a.price - b.price,
        _ => 0,
    };

    OrderBookSnapshot {
        pair,
        bids,
        asks,
        spread,
        last_price,
        last_update_round: current_round,
    }
}

/// Aggregate orders into price levels.
///
/// `ascending` = true sorts levels by ascending price (asks); false = descending (bids).
#[derive(Clone, Copy)]
struct ViewOrder {
    price: u64,
    amount: u64,
}

fn order_for_view(order: &OrderEntry, view_base_asset: u64) -> Option<ViewOrder> {
    if order.sell_asset != view_base_asset && order.buy_asset != view_base_asset {
        return None;
    }

    let creator_base = match order.side {
        opennodia_swap::OrderSide::Sell => order.sell_asset,
        opennodia_swap::OrderSide::Buy => order.buy_asset,
    };
    let price = if creator_base == view_base_asset {
        order.price
    } else {
        invert_price(order.price)
    };
    if price == 0 || order.sell_amount == 0 {
        return None;
    }

    let remaining_sell = order.remaining();
    let amount = if order.sell_asset == view_base_asset {
        remaining_sell
    } else {
        ((u128::from(order.buy_amount) * u128::from(remaining_sell))
            / u128::from(order.sell_amount))
        .min(u128::from(u64::MAX)) as u64
    };
    if amount == 0 {
        return None;
    }

    Some(ViewOrder { price, amount })
}

fn aggregate_levels(orders: &[ViewOrder], ascending: bool) -> Vec<PriceLevel> {
    // Group by price.
    let mut map: std::collections::BTreeMap<u64, (u64, usize)> = std::collections::BTreeMap::new();
    for o in orders {
        let entry = map.entry(o.price).or_insert((0, 0));
        entry.0 = entry.0.saturating_add(o.amount);
        entry.1 += 1;
    }

    let mut levels: Vec<PriceLevel> = map
        .into_iter()
        .map(|(price, (amount, count))| PriceLevel {
            price,
            amount,
            order_count: count,
            total: 0, // filled below
        })
        .collect();

    if !ascending {
        levels.reverse();
    }

    // Compute cumulative totals.
    let mut running: u64 = 0;
    for lvl in levels.iter_mut() {
        running = running.saturating_add(lvl.amount);
        lvl.total = running;
    }

    levels
}

/// Get recent trades for a pair.
pub fn get_recent_trades(db: &DexDb, pair: Pair, limit: u32) -> opennodia_core::Result<Vec<Trade>> {
    db.get_recent_trades(pair, limit)
}

/// Get trades for a specific account.
pub fn get_trades_for_account(
    db: &DexDb,
    addr: &opennodia_core::Address,
    limit: u32,
) -> opennodia_core::Result<Vec<Trade>> {
    db.get_trades_for_account(addr, limit)
}

/// Sweep expired orders: mark them in the DB and return the count.
pub fn sweep_expired(db: &DexDb, current_round: Round) -> opennodia_core::Result<u64> {
    db.mark_expired(current_round)
}

/// Re-export EntryStatus for convenience.
pub use crate::types::EntryStatus as Status;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::EntryStatus;
    use opennodia_core::Address;
    use opennodia_swap::{EscrowParams, OrderSide};

    fn entry(
        price: u64,
        amount: u64,
        side: OrderSide,
        sell_asset: u64,
        buy_asset: u64,
    ) -> OrderEntry {
        let (sell_amount, buy_amount) = match side {
            OrderSide::Sell => (
                amount,
                ((price as u128) * (amount as u128) / 1_000_000) as u64,
            ),
            OrderSide::Buy => (
                ((price as u128) * (amount as u128) / 1_000_000) as u64,
                amount,
            ),
        };
        OrderEntry {
            escrow_addr: Address::from_bytes([price as u8; 32]),
            side,
            sell_asset,
            sell_amount,
            buy_asset,
            buy_amount,
            price,
            owner: Address::from_bytes([1u8; 32]),
            created_round: Round(1000),
            expire_round: Round(100_000),
            status: EntryStatus::Active,
            filled_amount: 0,
            split_index: 0,
            parent_id: None,
            program: Vec::new(),
            params: EscrowParams::new(
                Address::from_bytes([1u8; 32]),
                sell_asset,
                sell_amount,
                buy_asset,
                buy_amount,
                100_000,
            ),
        }
    }

    #[test]
    fn asks_sorted_ascending() {
        let pair = Pair::new(0, 12345); // ALGO/ASA
        let orders = vec![
            entry(2_500_000, 100, OrderSide::Sell, 12345, 0),
            entry(2_000_000, 200, OrderSide::Sell, 12345, 0),
            entry(2_250_000, 150, OrderSide::Sell, 12345, 0),
        ];
        let snap = build_snapshot_from_orders(pair, 12345, &orders, Round(1000), None);
        assert_eq!(snap.asks.len(), 3);
        assert!(snap.asks[0].price <= snap.asks[1].price);
        assert!(snap.asks[1].price <= snap.asks[2].price);
        assert_eq!(snap.asks[0].price, 2_000_000); // cheapest first
    }

    #[test]
    fn bids_sorted_descending() {
        let pair = Pair::new(0, 12345);
        let orders = vec![
            entry(2_500_000, 100, OrderSide::Buy, 0, 12345),
            entry(2_000_000, 200, OrderSide::Buy, 0, 12345),
            entry(2_250_000, 150, OrderSide::Buy, 0, 12345),
        ];
        let snap = build_snapshot_from_orders(pair, 12345, &orders, Round(1000), None);
        assert_eq!(snap.bids.len(), 3);
        assert!(snap.bids[0].price >= snap.bids[1].price);
        assert!(snap.bids[1].price >= snap.bids[2].price);
        assert_eq!(snap.bids[0].price, 2_500_000); // highest first
    }

    #[test]
    fn cumulative_totals_correct() {
        let pair = Pair::new(0, 12345);
        let orders = vec![
            entry(2_000_000, 100, OrderSide::Sell, 12345, 0),
            entry(2_000_000, 200, OrderSide::Sell, 12345, 0),
            entry(2_500_000, 150, OrderSide::Sell, 12345, 0),
        ];
        let snap = build_snapshot_from_orders(pair, 12345, &orders, Round(1000), None);
        // Two levels: 2M (300 total) and 2.5M (450 total).
        assert_eq!(snap.asks.len(), 2);
        assert_eq!(snap.asks[0].amount, 300);
        assert_eq!(snap.asks[0].total, 300);
        assert_eq!(snap.asks[0].order_count, 2);
        assert_eq!(snap.asks[1].amount, 150);
        assert_eq!(snap.asks[1].total, 450);
    }

    #[test]
    fn spread_computed() {
        let pair = Pair::new(0, 12345);
        let orders = vec![
            entry(2_000_000, 100, OrderSide::Sell, 12345, 0), // best ask
            entry(1_900_000, 100, OrderSide::Buy, 0, 12345),  // best bid
        ];
        let snap = build_snapshot_from_orders(pair, 12345, &orders, Round(1000), None);
        // spread = 2_000_000 - 1_900_000 = 100_000
        assert_eq!(snap.spread, 100_000);
    }

    #[test]
    fn empty_orderbook() {
        let pair = Pair::new(0, 12345);
        let snap = build_snapshot_from_orders(pair, 12345, &[], Round(1000), None);
        assert!(snap.asks.is_empty());
        assert!(snap.bids.is_empty());
        assert_eq!(snap.spread, 0);
    }

    #[test]
    fn same_price_aggregated() {
        let pair = Pair::new(0, 12345);
        let orders = vec![
            entry(2_000_000, 100, OrderSide::Sell, 12345, 0),
            entry(2_000_000, 200, OrderSide::Sell, 12345, 0),
            entry(2_000_000, 300, OrderSide::Sell, 12345, 0),
        ];
        let snap = build_snapshot_from_orders(pair, 12345, &orders, Round(1000), None);
        assert_eq!(snap.asks.len(), 1);
        assert_eq!(snap.asks[0].amount, 600);
        assert_eq!(snap.asks[0].order_count, 3);
    }

    #[test]
    fn opposite_pair_perspectives_land_on_opposite_sides() {
        let pair = Pair::new(0, 12345);
        let orders = vec![
            entry(2_000_000, 100, OrderSide::Sell, 0, 12345),
            entry(500_000, 200, OrderSide::Sell, 12345, 0),
        ];

        let snap = build_snapshot_from_orders(pair, 0, &orders, Round(1000), None);

        assert_eq!(snap.asks.len(), 1);
        assert_eq!(snap.asks[0].price, 2_000_000);
        assert_eq!(snap.asks[0].amount, 100);
        assert_eq!(snap.bids.len(), 1);
        assert_eq!(snap.bids[0].price, 2_000_000);
        assert_eq!(snap.bids[0].amount, 100);
    }

    #[test]
    fn reciprocal_price_uses_micro_ratio_scale() {
        assert_eq!(invert_price(20_000), 50_000_000);
        assert_eq!(invert_price(50_000_000), 20_000);
        assert_eq!(invert_price(0), 0);
    }
}
