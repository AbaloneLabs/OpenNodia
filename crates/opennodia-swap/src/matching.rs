//! Smart routing and matching engine.
//!
//! The matching engine takes an incoming order request, scans the orderbook
//! for matching counterparties, and constructs atomic fill groups (up to 8
//! simultaneous fills = 16 transactions per Algorand atomic group).
//!
//! Key design decisions:
//! - LogicSig escrows are **fixed-amount**: partial fill of a single escrow is
//!   not possible. The engine fills whole escrows, skipping those larger than
//!   the remaining quantity.
//! - Each escrow TEAL uses relative `gtxn(GroupIndex - 1)` references (Phase 1
//!   v2), so batch fills work at any even group index.
//! - For >8 fills, multiple sequential atomic groups are emitted.

use opennodia_core::{Address, Round};
use serde::{Deserialize, Serialize};

use crate::create::split_amounts;
use crate::escrow::{EscrowAccount, EscrowKind, EscrowParams};
use crate::fill::{build_fill_group, derive_lease, FillResult};
use crate::order::OrderSide;
use crate::tx::{TransactionFields, TransactionParams};

/// Maximum fills per atomic group (16 tx / 2 tx per fill = 8 fills).
pub const MAX_FILLS_PER_GROUP: usize = 8;

/// Maximum splits for a standing order (fee/cost guardrail).
pub const MAX_SPLITS: u32 = 20;

/// An incoming order request to be matched against the orderbook.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderRequest {
    /// Side of the incoming order.
    pub side: OrderSide,
    /// ASA being sold (0 = ALGO).
    pub sell_asset: u64,
    /// ASA being bought (0 = ALGO).
    pub buy_asset: u64,
    /// Raw units to sell.
    pub sell_amount: u64,
    /// Raw units to buy.
    pub buy_amount: u64,
    /// Number of escrow splits for any remaining standing order.
    #[serde(default = "default_split_count")]
    pub split_count: u32,
    /// Whether to route against the orderbook first (true) or just issue a
    /// standing order (false).
    #[serde(default)]
    pub immediate_fill: bool,
    /// Owner address (seller for Sell, buyer for Buy).
    pub owner: Address,
    /// Expiry round for any new standing orders.
    pub expire_round: Round,
}

fn default_split_count() -> u32 {
    1
}

impl OrderRequest {
    /// The limit price (buy per sell unit) as a micro-ratio (×1e6).
    pub fn price_micro(&self) -> u64 {
        if self.sell_amount == 0 || self.buy_amount == 0 {
            return 0;
        }
        let (numerator, denominator) = match self.side {
            OrderSide::Sell => (self.buy_amount, self.sell_amount),
            OrderSide::Buy => (self.sell_amount, self.buy_amount),
        };
        let num = (numerator as u128) * 1_000_000u128;
        let den = denominator as u128;
        ((num / den).min(u64::MAX as u128)) as u64
    }
}

/// An orderbook entry for a single escrow.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BookOrder {
    pub escrow_address: Address,
    pub owner: Address,
    pub side: OrderSide,
    pub sell_asset: u64,
    pub sell_amount: u64,
    pub buy_asset: u64,
    pub buy_amount: u64,
    /// Current available amount in the escrow (may be < sell_amount if partial
    /// fills already occurred, though v1 escrows are atomic whole-fills).
    pub available_amount: u64,
    /// Compiled TEAL program with embedded params (template embedding, v2).
    #[serde(default)]
    pub program: Vec<u8>,
    /// Escrow params for re-derivation and fill construction.
    pub params: EscrowParams,
}

impl BookOrder {
    /// Price (buy per sell unit) as a micro-ratio (×1e6).
    pub fn price_micro(&self) -> u64 {
        if self.sell_amount == 0 || self.buy_amount == 0 {
            return 0;
        }
        let (numerator, denominator) = match self.side {
            OrderSide::Sell => (self.buy_amount, self.sell_amount),
            OrderSide::Buy => (self.sell_amount, self.buy_amount),
        };
        let num = (numerator as u128) * 1_000_000u128;
        let den = denominator as u128;
        ((num / den).min(u64::MAX as u128)) as u64
    }

    /// Build the escrow account from the stored params.
    pub fn to_escrow(&self) -> opennodia_core::Result<EscrowAccount> {
        let kind = match self.side {
            OrderSide::Sell => EscrowKind::Sell,
            OrderSide::Buy => EscrowKind::Buy,
        };
        EscrowAccount::from_program(kind, self.params.clone(), self.program.clone())
    }
}

/// Trait abstracting orderbook queries.
///
/// Phase 4 ships an in-memory stub for testing; Phase 5 backs this with SQLite.
pub trait OrderbookSource {
    /// Get active orders on the opposite side of the given trade.
    ///
    /// For a Buy request, returns Sell orders; for a Sell request, returns Buy
    /// orders. `max_price` filters by acceptable price (micro-ratio). Results
    /// are sorted best-price-first.
    fn get_opposite_orders(
        &self,
        side: OrderSide,
        sell_asset: u64,
        buy_asset: u64,
        max_price: Option<u64>,
    ) -> Vec<BookOrder>;
}

/// A single fill candidate selected by the matching engine.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FillCandidate {
    pub order: BookOrder,
    /// Amount of the *incoming* order's sell asset to fill (whole escrow).
    pub fill_amount: u64,
    /// Effective price (micro-ratio).
    pub price_micro: u64,
}

/// Aggregated statistics over a set of fills.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FillStats {
    /// Total cost in the buy asset (what the filler pays).
    pub total_cost: u64,
    /// Total received in the sell asset (what the filler gets).
    pub total_received: u64,
    /// Volume-weighted average price (micro-ratio).
    pub average_price: u64,
    /// Best (lowest for buyer) price across fills.
    pub best_price: u64,
    /// Worst (highest for buyer) price across fills.
    pub worst_price: u64,
}

/// Result of matching an order against the book.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MatchResult {
    /// Escrows to fill immediately.
    pub immediate_fills: Vec<FillCandidate>,
    /// Quantity left after fills (raw units of the incoming sell asset).
    pub remaining_amount: u64,
    /// How many new escrows to create for the remaining amount.
    pub remaining_splits: u32,
    /// Total buy asset spent on immediate fills.
    pub total_cost: u64,
    /// Total sell asset received from immediate fills.
    pub total_received: u64,
    /// Volume-weighted average price (micro-ratio).
    pub average_price: u64,
}

/// A plan to deposit a new standing escrow for the remaining amount.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EscrowDepositPlan {
    pub kind: EscrowKind,
    pub params: EscrowParams,
    pub deposit_amount: u64,
}

/// The routing decision for an order request.
#[derive(Clone, Debug)]
pub enum RoutingDecision {
    /// Fill from the book, then issue standing orders for the remainder.
    FillAndIssue {
        fills: Vec<FillCandidate>,
        fill_results: Vec<FillResult>,
        remaining_plans: Vec<EscrowDepositPlan>,
        stats: FillStats,
        /// Quantity that could not be matched, denominated in the incoming
        /// order's *sell* asset (the asset the filler pays with). This is the
        /// exact value returned by `match_order` and is the canonical source
        /// of truth for "how much was left over" — do not derive it by
        /// subtracting `stats.total_received` (different asset denomination).
        remaining_amount: u64,
    },
    /// Fully matched from the book; no standing orders needed.
    FillOnly {
        fills: Vec<FillCandidate>,
        fill_results: Vec<FillResult>,
        stats: FillStats,
    },
    /// No matching; just issue standing orders.
    IssueOnly { plans: Vec<EscrowDepositPlan> },
}

// ============================================================================
// Core matching algorithm
// ============================================================================

/// Match an order request against the orderbook.
///
/// Returns the fills to execute immediately and the remaining quantity to
/// issue as new standing escrows.
pub fn match_order(request: &OrderRequest, book: &dyn OrderbookSource) -> MatchResult {
    // The incoming order's acceptable price.
    let request_price = request.price_micro();

    // Get opposite-side orders. For a Buy request, we want Sell orders; for a
    // Sell request, we want Buy orders. The book query takes the *incoming*
    // side and returns the opposite.
    //
    // Price filter: for a buyer, accept sell orders priced <= request; for a
    // seller, accept buy orders priced >= request. We pass the request price
    // as max_price for buyers (cap what we pay), and None for sellers (we want
    // the highest bids, which the book sorts first).
    let max_price_filter = match request.side {
        OrderSide::Buy => Some(request_price),
        OrderSide::Sell => None,
    };

    let mut candidates = book.get_opposite_orders(
        request.side,
        request.sell_asset,
        request.buy_asset,
        max_price_filter,
    );

    // For a seller, filter bids below the acceptable price.
    if request.side == OrderSide::Sell {
        candidates.retain(|o| o.price_micro() >= request_price);
    }

    // Sort best-price-first.
    // - Buyer wants cheapest asks → ascending price.
    // - Seller wants highest bids → descending price.
    match request.side {
        OrderSide::Buy => candidates.sort_by_key(|o| o.price_micro()),
        OrderSide::Sell => candidates.sort_by_key(|o| std::cmp::Reverse(o.price_micro())),
    }

    let mut fills: Vec<FillCandidate> = Vec::new();
    // `remaining` tracks how much of the *incoming order's sell asset* (the
    // asset the filler pays with) the filler can still spend. Each matched
    // escrow consumes `order.buy_amount` of that asset (what the escrow owner
    // demands as payment), NOT `order.sell_amount` (which is denominated in the
    // opposite asset the filler receives).
    let mut remaining = request.sell_amount;
    let mut total_cost: u64 = 0;
    let mut total_received: u64 = 0;

    for order in candidates {
        if remaining == 0 {
            break;
        }
        if fills.len() >= MAX_FILLS_PER_GROUP {
            break;
        }
        // LogicSig escrows are fixed-amount: fill the whole escrow if the
        // filler can afford the required payment, skip otherwise.
        //
        // The escrow demands `order.buy_amount` of the filler's sell asset.
        // Compare in the filler's payment denomination, not the escrow's.
        let escrow_payment = order.buy_amount;
        if escrow_payment > remaining {
            continue;
        }
        // The filler receives `order.sell_amount` of the buy asset.
        let escrow_received = order.sell_amount;
        fills.push(FillCandidate {
            order: order.clone(),
            fill_amount: escrow_received,
            price_micro: order.price_micro(),
        });
        remaining = remaining.saturating_sub(escrow_payment);
        total_cost = total_cost.saturating_add(escrow_payment);
        total_received = total_received.saturating_add(escrow_received);
    }

    let average_price = if total_received == 0 {
        0
    } else {
        let num = (total_cost as u128) * 1_000_000u128;
        let den = total_received as u128;
        ((num / den).min(u64::MAX as u128)) as u64
    };

    let remaining_splits = if remaining == 0 {
        0
    } else {
        request.split_count.clamp(1, MAX_SPLITS)
    };

    MatchResult {
        immediate_fills: fills,
        remaining_amount: remaining,
        remaining_splits,
        total_cost,
        total_received,
        average_price,
    }
}

/// Compute aggregate statistics over a set of fill candidates.
pub fn compute_fill_stats(fills: &[FillCandidate]) -> FillStats {
    if fills.is_empty() {
        return FillStats::default();
    }
    let mut total_cost: u64 = 0;
    let mut total_received: u64 = 0;
    let mut best_price = u64::MAX;
    let mut worst_price: u64 = 0;
    // Volume-weighted average of per-fill price_micro, weighted by fill_amount.
    let mut weighted_price_sum: u128 = 0;
    for f in fills {
        total_cost = total_cost.saturating_add(f.order.buy_amount);
        total_received = total_received.saturating_add(f.fill_amount);
        best_price = best_price.min(f.price_micro);
        worst_price = worst_price.max(f.price_micro);
        weighted_price_sum =
            weighted_price_sum.saturating_add((f.fill_amount as u128) * (f.price_micro as u128));
    }
    let average_price = if total_received == 0 {
        0
    } else {
        ((weighted_price_sum / total_received as u128).min(u64::MAX as u128)) as u64
    };
    FillStats {
        total_cost,
        total_received,
        average_price,
        best_price,
        worst_price,
    }
}
/// Build standing-order deposit plans for the remaining quantity.
pub fn build_remaining_orders(
    remaining: u64,
    request: &OrderRequest,
    splits: u32,
) -> Vec<EscrowDepositPlan> {
    if remaining == 0 || splits == 0 {
        return Vec::new();
    }
    let kind = match request.side {
        OrderSide::Sell => EscrowKind::Sell,
        OrderSide::Buy => EscrowKind::Buy,
    };
    let amounts = split_amounts(remaining, splits);
    amounts
        .into_iter()
        .enumerate()
        .filter(|(_, amt)| *amt > 0)
        .map(|(i, amt)| {
            // Proportional buy amount for this split.
            let buy_amt = if request.sell_amount == 0 {
                0
            } else {
                let num = (request.buy_amount as u128) * (amt as u128);
                let den = request.sell_amount as u128;
                ((num / den).min(u64::MAX as u128)) as u64
            };
            let params = EscrowParams::new(
                request.owner,
                request.sell_asset,
                amt,
                request.buy_asset,
                buy_amt,
                request.expire_round.as_u64(),
            );
            // Use a unique note per split to avoid address collisions? No —
            // different amounts already produce different programs/addresses.
            let _ = i;
            EscrowDepositPlan {
                kind,
                params,
                deposit_amount: amt,
            }
        })
        .collect()
}

/// Build the atomic fill transaction groups for a set of fill candidates.
///
/// Returns one `FillResult` per candidate (each is a 2-tx group). For >8
/// fills, the caller must split into multiple sequential groups.
pub fn build_fill_groups(
    fills: &[FillCandidate],
    filler: Address,
    params: &TransactionParams,
) -> opennodia_core::Result<Vec<FillResult>> {
    fills
        .iter()
        .map(|f| {
            let escrow = f.order.to_escrow()?;
            let lease = derive_lease(filler, escrow.address);
            build_fill_group(&escrow, filler, lease, params)
        })
        .collect()
}

/// Flatten a set of fill groups into a single transaction list (for atomic
/// submission of up to 8 fills = 16 transactions).
///
/// Transactions are interleaved: [filler_0, escrow_0, filler_1, escrow_1, ...].
pub fn flatten_fill_groups(results: &[FillResult]) -> Vec<TransactionFields> {
    let mut out = Vec::new();
    for r in results {
        out.push(r.filler_tx.clone());
        out.extend(r.escrow_txs.clone());
    }
    out
}

/// Estimate the number of atomic groups needed for a fill count.
pub fn estimate_batch_count(fill_count: usize) -> usize {
    fill_count.div_ceil(MAX_FILLS_PER_GROUP)
}

/// Top-level routing: decide whether to fill, issue, or both.
///
/// Builds the actual fill transaction groups and remaining deposit plans.
pub fn route_order(
    request: &OrderRequest,
    book: &dyn OrderbookSource,
    filler: Address,
    params: &TransactionParams,
) -> opennodia_core::Result<RoutingDecision> {
    if !request.immediate_fill {
        // Issue-only: skip matching entirely.
        let plans = build_remaining_orders(request.sell_amount, request, request.split_count);
        return Ok(RoutingDecision::IssueOnly { plans });
    }

    let matched = match_order(request, book);

    if matched.immediate_fills.is_empty() {
        // No matches: issue the full amount as standing orders.
        let plans = build_remaining_orders(request.sell_amount, request, request.split_count);
        return Ok(RoutingDecision::IssueOnly { plans });
    }

    let fill_results = build_fill_groups(&matched.immediate_fills, filler, params)?;
    let stats = compute_fill_stats(&matched.immediate_fills);

    if matched.remaining_amount == 0 {
        Ok(RoutingDecision::FillOnly {
            fills: matched.immediate_fills,
            fill_results,
            stats,
        })
    } else {
        let remaining_plans =
            build_remaining_orders(matched.remaining_amount, request, matched.remaining_splits);
        Ok(RoutingDecision::FillAndIssue {
            fills: matched.immediate_fills,
            fill_results,
            remaining_plans,
            stats,
            remaining_amount: matched.remaining_amount,
        })
    }
}

/// Format a human-readable fill preview.
pub fn format_fill_preview(stats: &FillStats, decimals: u32) -> String {
    let divisor = 10u64.pow(decimals);
    let recv = stats.total_received as f64 / divisor as f64;
    let cost = stats.total_cost as f64 / divisor as f64;
    let avg = stats.average_price as f64 / 1_000_000.0;
    format!(
        "Fill {recv} for {cost} (avg {avg}, best {})",
        stats.best_price as f64 / 1_000_000.0
    )
}

// ============================================================================
// In-memory orderbook (testing stub)
// ============================================================================

/// A simple in-memory orderbook for testing and Phase 4 stand-alone use.
#[derive(Default)]
pub struct InMemoryOrderbook {
    orders: Vec<BookOrder>,
}

impl InMemoryOrderbook {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an order to the book.
    pub fn add(&mut self, order: BookOrder) {
        self.orders.push(order);
    }

    /// Number of orders in the book.
    pub fn len(&self) -> usize {
        self.orders.len()
    }

    /// Whether the book is empty.
    pub fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }
}

impl OrderbookSource for InMemoryOrderbook {
    fn get_opposite_orders(
        &self,
        side: OrderSide,
        sell_asset: u64,
        buy_asset: u64,
        max_price: Option<u64>,
    ) -> Vec<BookOrder> {
        let opposite_side = match side {
            OrderSide::Buy => OrderSide::Sell,
            OrderSide::Sell => OrderSide::Buy,
        };
        self.orders
            .iter()
            .filter(|o| {
                // Cross the trading pair: an incoming order that sells A for B
                // matches an opposite order that sells B for A.
                if side == OrderSide::Buy {
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
                }
            })
            .filter(|o| match max_price {
                Some(mp) => o.price_micro() <= mp,
                None => true,
            })
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner_a() -> Address {
        Address::from_bytes([1u8; 32])
    }
    fn owner_b() -> Address {
        Address::from_bytes([2u8; 32])
    }

    fn make_sell_order(
        sell_asset: u64,
        sell_amount: u64,
        buy_asset: u64,
        buy_amount: u64,
        owner: Address,
    ) -> BookOrder {
        let params = EscrowParams::new(
            owner,
            sell_asset,
            sell_amount,
            buy_asset,
            buy_amount,
            100_000,
        );
        let acct = EscrowAccount::from_program(
            EscrowKind::Sell,
            params.clone(),
            vec![
                sell_asset as u8,
                sell_amount as u8,
                buy_asset as u8,
                buy_amount as u8,
                1,
            ],
        )
        .unwrap();
        BookOrder {
            escrow_address: acct.address,
            owner,
            side: OrderSide::Sell,
            sell_asset,
            sell_amount,
            buy_asset,
            buy_amount,
            available_amount: sell_amount,
            program: acct.program,
            params,
        }
    }

    fn tx_params() -> TransactionParams {
        TransactionParams::new(Round(1000), "testnet-v1.0".into(), [0xaa; 32])
    }

    #[test]
    fn empty_orderbook_all_remaining() {
        let book = InMemoryOrderbook::new();
        let req = OrderRequest {
            side: OrderSide::Buy,
            sell_asset: 0,
            buy_asset: 12345,
            sell_amount: 1_000,
            buy_amount: 2_000_000,
            split_count: 1,
            immediate_fill: true,
            owner: owner_b(),
            expire_round: Round(100_000),
        };
        let res = match_order(&req, &book);
        assert!(res.immediate_fills.is_empty());
        assert_eq!(res.remaining_amount, 1_000);
    }

    #[test]
    fn single_match_then_remaining() {
        let mut book = InMemoryOrderbook::new();
        // Sell order: 400 units of asset 12345 for 800 ALGO (800_000 micro).
        book.add(make_sell_order(12345, 400, 0, 800_000, owner_a()));

        // Buy request: pay 1_000 ALGO (1_000_000 micro) for asset 12345.
        // The filler can afford the 800_000 micro payment for this escrow.
        let req = OrderRequest {
            side: OrderSide::Buy,
            sell_asset: 0,
            buy_asset: 12345,
            sell_amount: 1_000_000,
            buy_amount: 500,
            split_count: 1,
            immediate_fill: true,
            owner: owner_b(),
            expire_round: Round(100_000),
        };
        let res = match_order(&req, &book);
        assert_eq!(res.immediate_fills.len(), 1);
        // Filler receives the escrow's full sell amount (400 asset units).
        assert_eq!(res.immediate_fills[0].fill_amount, 400);
        // Filler paid 800_000 micro; remaining budget is 1_000_000 - 800_000.
        assert_eq!(res.remaining_amount, 200_000);
        assert_eq!(res.total_received, 400);
        assert_eq!(res.total_cost, 800_000);
    }

    #[test]
    fn multiple_matches_batch_fill() {
        let mut book = InMemoryOrderbook::new();
        // Three sell orders, each 300 asset units for 600_000 micro ALGO.
        book.add(make_sell_order(12345, 300, 0, 600_000, owner_a()));
        book.add(make_sell_order(12345, 300, 0, 600_000, owner_a()));
        book.add(make_sell_order(12345, 300, 0, 600_000, owner_a()));

        // Buy request: pay 2_500_000 micro ALGO for asset 12345.
        // Can afford all three escrows (3 × 600_000 = 1_800_000).
        let req = OrderRequest {
            side: OrderSide::Buy,
            sell_asset: 0,
            buy_asset: 12345,
            sell_amount: 2_500_000,
            buy_amount: 1_000,
            split_count: 1,
            immediate_fill: true,
            owner: owner_b(),
            expire_round: Round(100_000),
        };
        let res = match_order(&req, &book);
        assert_eq!(res.immediate_fills.len(), 3);
        assert_eq!(res.total_received, 900);
        // 2_500_000 - 1_800_000 = 700_000 micro remaining.
        assert_eq!(res.remaining_amount, 700_000);
    }

    /// Regression test: cross-asset denomination bug. The matching engine must
    /// compare the filler's remaining *payment budget* (in the filler's sell
    /// asset) against the escrow's demanded payment (buy_amount), NOT against
    /// the escrow's sell_amount (denominated in the opposite asset).
    #[test]
    fn cross_asset_denomination_matching() {
        let mut book = InMemoryOrderbook::new();
        // Sell order: 10_000_000 QAT for 200_000 micro ALGO.
        book.add(make_sell_order(
            764766481,
            10_000_000,
            0,
            200_000,
            owner_a(),
        ));

        // Buy request: pay exactly 200_000 micro ALGO for QAT.
        let req = OrderRequest {
            side: OrderSide::Buy,
            sell_asset: 0,          // ALGO
            buy_asset: 764766481,   // QAT
            sell_amount: 200_000,   // micro ALGO budget
            buy_amount: 10_000_000, // QAT wanted
            split_count: 1,
            immediate_fill: true,
            owner: owner_b(),
            expire_round: Round(100_000),
        };
        let res = match_order(&req, &book);
        // Before the fix, the engine compared 10_000_000 (QAT) > 200_000 (ALGO)
        // and skipped the escrow. After the fix, it compares the payment
        // (200_000 micro ALGO) against the budget (200_000) and matches.
        assert_eq!(
            res.immediate_fills.len(),
            1,
            "cross-asset escrow should match when payment fits budget"
        );
        assert_eq!(res.total_received, 10_000_000);
        assert_eq!(res.total_cost, 200_000);
        assert_eq!(res.remaining_amount, 0);
    }

    #[test]
    fn price_filter_rejects_unfavorable() {
        let mut book = InMemoryOrderbook::new();
        // Expensive sell: 100 units for 10_000_000 ALGO (price 100_000 micro).
        book.add(make_sell_order(12345, 100, 0, 10_000_000, owner_a()));

        // Buy request willing to pay only 1_000_000 for 100 units (price 10_000).
        let req = OrderRequest {
            side: OrderSide::Buy,
            sell_asset: 0,
            buy_asset: 12345,
            sell_amount: 100,
            buy_amount: 1_000_000,
            split_count: 1,
            immediate_fill: true,
            owner: owner_b(),
            expire_round: Round(100_000),
        };
        let res = match_order(&req, &book);
        assert!(
            res.immediate_fills.is_empty(),
            "expensive order should be rejected"
        );
    }

    #[test]
    fn fill_limit_eight_respected() {
        let mut book = InMemoryOrderbook::new();
        // 10 sell orders, each 100 asset units for 200_000 micro ALGO.
        for _ in 0..10 {
            book.add(make_sell_order(12345, 100, 0, 200_000, owner_a()));
        }
        // Buy request with a large enough budget to afford all 10 escrows
        // (10 × 200_000 = 2_000_000 micro), but only 8 can be filled per group.
        let req = OrderRequest {
            side: OrderSide::Buy,
            sell_asset: 0,
            buy_asset: 12345,
            sell_amount: 10_000_000,
            buy_amount: 5_000,
            split_count: 1,
            immediate_fill: true,
            owner: owner_b(),
            expire_round: Round(100_000),
        };
        let res = match_order(&req, &book);
        assert_eq!(res.immediate_fills.len(), MAX_FILLS_PER_GROUP);
    }

    #[test]
    fn estimate_batch_count_correct() {
        assert_eq!(estimate_batch_count(0), 0);
        assert_eq!(estimate_batch_count(1), 1);
        assert_eq!(estimate_batch_count(8), 1);
        assert_eq!(estimate_batch_count(9), 2);
        assert_eq!(estimate_batch_count(16), 2);
        assert_eq!(estimate_batch_count(17), 3);
    }

    #[test]
    fn remaining_split_even() {
        let req = OrderRequest {
            side: OrderSide::Buy,
            sell_asset: 0,
            buy_asset: 12345,
            sell_amount: 1_000,
            buy_amount: 2_000_000,
            split_count: 4,
            immediate_fill: false,
            owner: owner_b(),
            expire_round: Round(100_000),
        };
        let plans = build_remaining_orders(1_000, &req, 4);
        assert_eq!(plans.len(), 4);
        let sum: u64 = plans.iter().map(|p| p.deposit_amount).sum();
        assert_eq!(sum, 1_000);
    }

    #[test]
    fn remaining_split_uneven_distributes_remainder() {
        let req = OrderRequest {
            side: OrderSide::Buy,
            sell_asset: 0,
            buy_asset: 12345,
            sell_amount: 1_000,
            buy_amount: 2_000_000,
            split_count: 3,
            immediate_fill: false,
            owner: owner_b(),
            expire_round: Round(100_000),
        };
        let plans = build_remaining_orders(1_000, &req, 3);
        assert_eq!(plans.len(), 3);
        // 1000 / 3 = 333 remainder 1 → first split gets 334.
        assert_eq!(plans[0].deposit_amount, 334);
        assert_eq!(plans[1].deposit_amount, 333);
        assert_eq!(plans[2].deposit_amount, 333);
    }

    #[test]
    fn route_issue_only_when_no_immediate() {
        let book = InMemoryOrderbook::new();
        let req = OrderRequest {
            side: OrderSide::Buy,
            sell_asset: 0,
            buy_asset: 12345,
            sell_amount: 1_000,
            buy_amount: 2_000_000,
            split_count: 2,
            immediate_fill: false,
            owner: owner_b(),
            expire_round: Round(100_000),
        };
        match route_order(&req, &book, owner_b(), &tx_params()).unwrap() {
            RoutingDecision::IssueOnly { plans } => {
                assert_eq!(plans.len(), 2);
            }
            other => panic!("expected IssueOnly, got {other:?}"),
        }
    }

    #[test]
    fn route_fill_only_when_fully_matched() {
        let mut book = InMemoryOrderbook::new();
        book.add(make_sell_order(12345, 1_000, 0, 2_000_000, owner_a()));
        let req = OrderRequest {
            side: OrderSide::Buy,
            sell_asset: 0,
            buy_asset: 12345,
            sell_amount: 2_000_000,
            buy_amount: 1_000,
            split_count: 1,
            immediate_fill: true,
            owner: owner_b(),
            expire_round: Round(100_000),
        };
        match route_order(&req, &book, owner_b(), &tx_params()).unwrap() {
            RoutingDecision::FillOnly { fills, stats, .. } => {
                assert_eq!(fills.len(), 1);
                assert_eq!(stats.total_received, 1_000);
            }
            other => panic!("expected FillOnly, got {other:?}"),
        }
    }

    #[test]
    fn route_fill_and_issue_when_partial() {
        let mut book = InMemoryOrderbook::new();
        book.add(make_sell_order(12345, 400, 0, 800_000, owner_a()));
        let req = OrderRequest {
            side: OrderSide::Buy,
            sell_asset: 0,
            buy_asset: 12345,
            sell_amount: 1_000_000,
            buy_amount: 500,
            split_count: 2,
            immediate_fill: true,
            owner: owner_b(),
            expire_round: Round(100_000),
        };
        match route_order(&req, &book, owner_b(), &tx_params()).unwrap() {
            RoutingDecision::FillAndIssue {
                fills,
                remaining_plans,
                ..
            } => {
                assert_eq!(fills.len(), 1);
                assert!(!remaining_plans.is_empty());
                let sum: u64 = remaining_plans.iter().map(|p| p.deposit_amount).sum();
                assert_eq!(sum, 200_000); // 1,000,000 - 800,000 payment budget
            }
            other => panic!("expected FillAndIssue, got {other:?}"),
        }
    }

    #[test]
    fn fill_stats_aggregate_correctly() {
        let fills = vec![
            FillCandidate {
                order: make_sell_order(12345, 100, 0, 200_000, owner_a()),
                fill_amount: 100,
                price_micro: 2_000_000,
            },
            FillCandidate {
                order: make_sell_order(12345, 200, 0, 500_000, owner_a()),
                fill_amount: 200,
                price_micro: 2_500_000,
            },
        ];
        let stats = compute_fill_stats(&fills);
        assert_eq!(stats.total_received, 300);
        assert_eq!(stats.total_cost, 700_000);
        assert_eq!(stats.best_price, 2_000_000);
        assert_eq!(stats.worst_price, 2_500_000);
        // avg = 700_000 / 300 * 1e6 = 2_333_333
        assert_eq!(stats.average_price, 2_333_333);
    }

    #[test]
    fn flatten_interleaves_filler_and_escrow() {
        let mut book = InMemoryOrderbook::new();
        book.add(make_sell_order(12345, 100, 0, 200_000, owner_a()));
        book.add(make_sell_order(12345, 100, 0, 200_000, owner_a()));
        let req = OrderRequest {
            side: OrderSide::Buy,
            sell_asset: 0,
            buy_asset: 12345,
            sell_amount: 400_000,
            buy_amount: 200,
            split_count: 1,
            immediate_fill: true,
            owner: owner_b(),
            expire_round: Round(100_000),
        };
        let matched = match_order(&req, &book);
        let results = build_fill_groups(&matched.immediate_fills, owner_b(), &tx_params()).unwrap();
        let flat = flatten_fill_groups(&results);
        assert_eq!(flat.len(), 6); // 2 ASA fills × 3 transactions
                                   // Grouped: filler payment, escrow release, escrow close-out.
        assert_eq!(flat[0].sender, owner_b()); // filler
        assert_ne!(flat[1].sender, owner_b()); // escrow
        assert_eq!(flat[3].sender, owner_b()); // next filler
    }
}
