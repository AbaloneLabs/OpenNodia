//! DEX API endpoints (Phase 6).
//!
//! All write endpoints use a prepare/submit pattern. Prepare stores an exact,
//! short-lived, session-bound transaction intent for review. Submit verifies
//! the wallet PIN, signs those exact transactions through KMD, and then sends
//! the atomic group to algod.
//!
//! Endpoints (all session-authenticated):
//! - `POST /api/dex/prepare/create` — build a deposit group for a new order
//! - `POST /api/dex/submit/create`  — relay a signed deposit group
//! - `POST /api/dex/prepare/fill`   — build a fill group for one escrow
//! - `POST /api/dex/submit/fill`    — relay a signed fill group
//! - `POST /api/dex/prepare/cancel` — build a cancel close-out group
//! - `POST /api/dex/submit/cancel`  — relay a signed cancel group
//! - `POST /api/dex/routes`         — unified quote-only route candidates
//! - `POST /api/dex/prepare/route`  — smart-routing preview (IOC fill plan)
//! - `POST /api/dex/submit/route`   — sign and submit routed fill groups
//! - `GET  /api/dex/orderbook`      — orderbook snapshot for a pair
//! - `GET  /api/dex/pairs`          — popular trading pairs (ranked)
//! - `GET  /api/dex/orders`         — the caller's orders
//! - `GET  /api/dex/trades`         — recent trades
//! - `GET  /api/dex/order/:escrow`  — single order + on-chain verification
//! - `GET  /api/dex/order/:escrow/link` — shareable order link for a local order
//! - `GET  /api/dex/order-link/:payload` — decoded link + canonical ledger verification

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use std::collections::HashSet;
use std::time::Duration;

use crate::api_error::{
    bad_request, internal, not_found, service_unavailable, ApiErrorResponse, ApiResult,
};
use opennodia_core::{Address, Round};
use opennodia_dex::types::{EntryStatus, OrderEntry, Pair, Trade};
#[cfg(test)]
use opennodia_dex::DexOrderbookSource;
use opennodia_node::AlgodClient;
use opennodia_swap::{
    decode_order_link, verify_escrow, CreateOrderResult, EscrowAccount, EscrowKind, EscrowParams,
    FillResult, OrderSide, OrderVerification, TransactionFields, TransactionParams,
};

use crate::intent::IntentStoreError;
use crate::session::Session;
use crate::state::AppState;
use crate::tx_flow::{self, TxDescription};

mod asset_policy;
mod dto;
mod order_handlers;
mod order_link;
mod orderbook_depth;
mod read_handlers;
mod reconcile;
mod route_handlers;
mod route_quotes;
use asset_policy::{reject_escrow_regulated_assets, reject_regulated_asset};
pub use dto::*;
use dto::{assign_synthetic_totals, default_expire_rounds, default_slippage_bps};
use order_handlers::{
    prepare_cancel, prepare_create, prepare_create_orders_from_plans, prepare_fill, submit_cancel,
    submit_create, submit_fill, submit_prepared_create_order,
};
use order_link::{
    order_entry_from_link_payload, order_link_payload_from_entry, order_link_response_from_payload,
};
#[cfg(test)]
use orderbook_depth::synthetic_level_from_candidate;
use orderbook_depth::synthetic_orderbook_depth;
use read_handlers::{
    dex_status, my_orders, order_detail, order_link_detail, order_link_for_order, orderbook,
    popular_pairs, trades,
};
pub use reconcile::reconcile_orders;
pub(crate) use route_handlers::{prepare_route, submit_route};
pub(crate) use route_quotes::route_candidates;
#[cfg(test)]
use route_quotes::{orderbook_route_candidate, OrderbookRouteContext};

#[derive(Clone)]
pub(crate) enum DexIntentAction {
    Create {
        orders: Vec<CreateIntentOrder>,
    },
    Fill {
        escrow: EscrowAccount,
        filler: Address,
        result: FillResult,
    },
    Cancel {
        escrow: EscrowAccount,
        result: opennodia_swap::CancelResult,
    },
    /// Routed order: one or more matched escrows to fill, optionally followed
    /// by standing child escrows for the unmatched remainder.
    Route {
        fills: Vec<RouteFill>,
        filler: Address,
        /// Discarded remainder (sell-asset units). Zero when the remainder is
        /// placed as standing child escrows.
        remaining: u64,
        creates: Vec<CreateIntentOrder>,
    },
}

#[derive(Clone)]
pub(crate) struct CreateIntentOrder {
    escrow: EscrowAccount,
    result: CreateOrderResult,
    split_index: u32,
    parent_id: Option<String>,
}

/// A single matched escrow within an IOC route intent.
#[derive(Clone)]
pub(crate) struct RouteFill {
    escrow: EscrowAccount,
    result: FillResult,
}

// ============================================================================
// Router
// ============================================================================

/// Build the DEX sub-router. Mounted under the protected (auth) layer.
///
/// Returns `Router<AppState>` (without state bound) so it can be merged into
/// the parent router before the final `.with_state()` call. The auth middleware
/// is applied by the parent router's protected layer.
pub fn dex_router() -> Router<AppState> {
    Router::new()
        .route("/api/dex/prepare/create", post(prepare_create))
        .route("/api/dex/submit/create", post(submit_create))
        .route("/api/dex/prepare/fill", post(prepare_fill))
        .route("/api/dex/submit/fill", post(submit_fill))
        .route("/api/dex/prepare/cancel", post(prepare_cancel))
        .route("/api/dex/submit/cancel", post(submit_cancel))
        .route("/api/dex/routes", post(route_candidates))
        .route("/api/dex/prepare/route", post(prepare_route))
        .route("/api/dex/submit/route", post(submit_route))
        .route("/api/dex/orderbook", get(orderbook))
        .route("/api/dex/pairs", get(popular_pairs))
        .route("/api/dex/status", get(dex_status))
        .route("/api/dex/orders", get(my_orders))
        .route("/api/dex/trades", get(trades))
        .route("/api/dex/order-link/{payload}", get(order_link_detail))
        .route("/api/dex/order/{escrow}/link", get(order_link_for_order))
        .route("/api/dex/order/{escrow}", get(order_detail))
}

// ============================================================================
// Helpers
// ============================================================================

/// Require the DEX store to be available.
fn require_dex(state: &AppState) -> ApiResult<std::sync::Arc<opennodia_dex::DexDb>> {
    state
        .stores
        .dex
        .clone()
        .ok_or_else(|| service_unavailable("DEX orderbook database unavailable"))
}

fn require_dex_write(state: &AppState) -> ApiResult<()> {
    let validation = state.runtime.dex_validation.snapshot();
    if !validation.allows_writes() {
        let reason = validation
            .error
            .unwrap_or_else(|| format!("runtime validation is {:?}", validation.phase));
        return Err(service_unavailable(format!(
            "DEX transaction writes are disabled: {reason}"
        )));
    }
    Ok(())
}

fn api_error(error: &ApiErrorResponse) -> String {
    error.1.error.clone()
}

#[cfg(test)]
fn quote_orderbook_bids(
    bids: &[opennodia_dex::types::PriceLevel],
    amount_in: u64,
) -> Option<(u64, u64, u64)> {
    if amount_in == 0 || bids.is_empty() {
        return None;
    }
    let best_price = bids.first()?.price;
    if best_price == 0 {
        return None;
    }

    let mut remaining = amount_in;
    let mut input_consumed = 0u64;
    let mut amount_out = 0u64;
    for level in bids {
        if remaining == 0 {
            break;
        }
        if level.price == 0 || level.amount == 0 {
            continue;
        }
        let take_in = remaining.min(level.amount);
        let level_out = mul_div_floor(take_in, level.price, 1_000_000);
        if level_out == 0 {
            break;
        }
        remaining = remaining.saturating_sub(take_in);
        input_consumed = input_consumed.saturating_add(take_in);
        amount_out = amount_out.saturating_add(level_out);
    }
    if input_consumed == 0 || amount_out == 0 {
        return None;
    }

    let average_price = mul_div_floor(amount_out, 1_000_000, input_consumed);
    let price_impact_bps = if average_price >= best_price {
        0
    } else {
        mul_div_floor(best_price - average_price, 10_000, best_price)
    };

    Some((amount_out, input_consumed, price_impact_bps))
}

fn mul_div_floor(value: u64, numerator: u64, denominator: u64) -> u64 {
    if denominator == 0 {
        return 0;
    }
    ((u128::from(value) * u128::from(numerator)) / u128::from(denominator))
        .min(u128::from(u64::MAX)) as u64
}

async fn store_intent(
    state: &AppState,
    session: &Session,
    wallet_id: &str,
    action: DexIntentAction,
) -> ApiResult<String> {
    let ttl = Duration::from_secs(state.config.dex.intent_ttl_secs.max(30));
    state
        .intents
        .dex
        .store(&session.sid, wallet_id, ttl, action)
        .await
        .map_err(intent_store_error)
}

async fn take_intent(
    state: &AppState,
    session: &Session,
    wallet_id: &str,
    intent_id: &str,
) -> ApiResult<DexIntentAction> {
    state
        .intents
        .dex
        .take(&session.sid, wallet_id, intent_id)
        .await
        .map_err(intent_store_error)
}

fn intent_store_error(error: IntentStoreError) -> ApiErrorResponse {
    crate::api_error::intent_store_error(error, "DEX")
}

fn escrow_from_entry(entry: &OrderEntry) -> ApiResult<EscrowAccount> {
    let kind = match entry.side {
        OrderSide::Sell => EscrowKind::Sell,
        OrderSide::Buy => EscrowKind::Buy,
    };
    let escrow = EscrowAccount::from_program(kind, entry.params.clone(), entry.program.clone())
        .map_err(|error| internal(format!("stored escrow program: {error}")))?;
    if escrow.address != entry.escrow_addr {
        return Err(internal("stored escrow address does not match its program"));
    }
    Ok(escrow)
}

async fn require_canonical_escrow(algod: &AlgodClient, escrow: &EscrowAccount) -> ApiResult<()> {
    let expected = EscrowAccount::compile(algod, escrow.kind, escrow.params.clone())
        .await
        .map_err(|error| service_unavailable(format!("compile canonical escrow: {error}")))?;
    if expected.program != escrow.program || expected.address != escrow.address {
        return Err(internal(
            "stored escrow program does not match the canonical generated program",
        ));
    }
    Ok(())
}

async fn canonical_escrow_from_entry(
    algod: &AlgodClient,
    entry: &OrderEntry,
) -> ApiResult<EscrowAccount> {
    let escrow = escrow_from_entry(entry)?;
    require_canonical_escrow(algod, &escrow).await?;
    Ok(escrow)
}

/// Parse an Algorand address from a base32 string.
fn parse_address(s: &str) -> ApiResult<Address> {
    s.parse::<Address>()
        .map_err(|e| bad_request(format!("invalid address '{s}': {e}")))
}

/// Resolve the historical `wallet_id` query parameter into owner addresses.
///
/// Older callers pass an owner address in `wallet_id`. API clients may also
/// pass an actual registered wallet ID, in which case all registered wallet
/// addresses are queried.
async fn resolve_order_owners(
    state: &AppState,
    wallet_or_address: &str,
) -> ApiResult<Vec<Address>> {
    if let Ok(address) = wallet_or_address.parse::<Address>() {
        return Ok(vec![address]);
    }

    let wallet = state
        .stores.wallets
        .list_wallets()
        .await
        .into_iter()
        .find(|wallet| wallet.id == wallet_or_address)
        .ok_or_else(|| {
            bad_request(format!(
                "invalid wallet_id '{wallet_or_address}'; expected a registered wallet ID or owner address"
            ))
        })?;

    let mut candidates = wallet.addresses;
    candidates.push(wallet.first_address);

    let mut seen = HashSet::new();
    let mut owners = Vec::new();
    for candidate in candidates {
        if !seen.insert(candidate.clone()) {
            continue;
        }
        owners.push(candidate.parse::<Address>().map_err(|error| {
            internal(format!(
                "registered wallet contains invalid address '{candidate}': {error}"
            ))
        })?);
    }

    if owners.is_empty() {
        return Err(not_found(format!(
            "wallet has no registered addresses: {wallet_or_address}"
        )));
    }

    Ok(owners)
}

/// Parse the escrow kind from a string.
fn parse_kind(s: &str) -> ApiResult<EscrowKind> {
    match s.to_ascii_lowercase().as_str() {
        "sell" => Ok(EscrowKind::Sell),
        "buy" => Ok(EscrowKind::Buy),
        _ => Err(bad_request(format!(
            "invalid kind '{s}'; expected 'sell' or 'buy'"
        ))),
    }
}

/// Parse an order side string ("sell"/"buy") into the matching `OrderSide`.
fn parse_side(s: &str) -> ApiResult<OrderSide> {
    match parse_kind(s)? {
        EscrowKind::Sell => Ok(OrderSide::Sell),
        EscrowKind::Buy => Ok(OrderSide::Buy),
    }
}

/// Verify that the caller's wallet owns the given address.
///
/// Requires PIN to unlock the wallet handle for address enumeration.
/// Prevents unauthorized cancellation of other users' orders.
async fn require_wallet_ownership(
    state: &AppState,
    wallet_id: &str,
    pin: &str,
    address: Address,
) -> ApiResult<()> {
    if !state.stores.wallets.contains_wallet(wallet_id).await {
        return Err(not_found(format!("wallet not found: {wallet_id}")));
    }

    let normalized = address.to_string();
    let belongs = state
        .stores
        .wallets
        .contains_address(wallet_id, pin, &normalized)
        .await
        .map_err(|e| service_unavailable(format!("list wallet addresses: {e}")))?;

    if !belongs {
        return Err(not_found(format!(
            "address does not belong to wallet: {normalized}"
        )));
    }

    Ok(())
}

/// Fetch suggested transaction parameters from algod.
async fn fetch_params(algod: &AlgodClient) -> ApiResult<TransactionParams> {
    opennodia_swap::fetch_tx_params(algod)
        .await
        .map_err(|e| service_unavailable(format!("fetch tx params: {e}")))
}

/// Build a TxDescription from a TransactionFields.
fn describe_tx(tx: &TransactionFields, signer_label: &str) -> TxDescription {
    tx_flow::describe_tx(tx, signer_label)
}

/// Register a confirmed order in the DexStore.
fn register_order(
    db: &opennodia_dex::DexDb,
    escrow: &EscrowAccount,
    created_round: Round,
    split_index: u32,
    parent_id: Option<String>,
) -> ApiResult<()> {
    let side = match escrow.kind {
        EscrowKind::Sell => OrderSide::Sell,
        EscrowKind::Buy => OrderSide::Buy,
    };
    let price = opennodia_dex::types::order_price(
        side,
        escrow.params.sell_asset,
        escrow.params.sell_amount,
        escrow.params.buy_asset,
        escrow.params.buy_amount,
    )
    .ok_or_else(|| bad_request("order price cannot be normalized"))?;
    let entry = OrderEntry {
        escrow_addr: escrow.address,
        side,
        sell_asset: escrow.params.sell_asset,
        sell_amount: escrow.params.sell_amount,
        buy_asset: escrow.params.buy_asset,
        buy_amount: escrow.params.buy_amount,
        price,
        owner: escrow.params.owner,
        created_round,
        expire_round: Round(escrow.params.expire_round),
        status: EntryStatus::Active,
        filled_amount: 0,
        split_index,
        parent_id,
        program: escrow.program.clone(),
        params: escrow.params.clone(),
    };
    db.register_order(&entry)
        .map_err(|e| internal(format!("register order: {e}")))
}

fn confirmed_trade(
    escrow: &EscrowAccount,
    filler: Address,
    tx_id: String,
    confirmed_round: u64,
    timestamp: u64,
) -> ApiResult<Trade> {
    let side = match escrow.kind {
        EscrowKind::Sell => OrderSide::Sell,
        EscrowKind::Buy => OrderSide::Buy,
    };
    let price = opennodia_dex::types::order_price(
        side,
        escrow.params.sell_asset,
        escrow.params.sell_amount,
        escrow.params.buy_asset,
        escrow.params.buy_amount,
    )
    .ok_or_else(|| internal("filled order price cannot be normalized"))?;
    let (buyer, seller) = match escrow.kind {
        EscrowKind::Sell => (filler, escrow.params.owner),
        EscrowKind::Buy => (escrow.params.owner, filler),
    };
    Ok(Trade {
        tx_id,
        pair: Pair::new(escrow.params.sell_asset, escrow.params.buy_asset),
        side,
        price,
        base_asset: Some(match side {
            OrderSide::Sell => escrow.params.sell_asset,
            OrderSide::Buy => escrow.params.buy_asset,
        }),
        amount: match side {
            OrderSide::Sell => escrow.params.sell_amount,
            OrderSide::Buy => escrow.params.buy_amount,
        },
        buyer,
        seller,
        round: Round(confirmed_round),
        timestamp,
    })
}

fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

// ============================================================================
// Handlers
// ============================================================================

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::read_handlers::parse_pair_str;
    use super::*;

    #[test]
    fn parse_kind_works() {
        assert_eq!(parse_kind("sell").unwrap(), EscrowKind::Sell);
        assert_eq!(parse_kind("Buy").unwrap(), EscrowKind::Buy);
        assert!(parse_kind("x").is_err());
    }

    #[test]
    fn parse_pair_str_works() {
        assert_eq!(parse_pair_str("0:12345").unwrap(), (0, 12345));
        assert!(parse_pair_str("nope").is_err());
        assert!(parse_pair_str("1:2:3").is_err());
    }

    #[test]
    fn order_response_from_entry() {
        let owner = Address::from_bytes([1u8; 32]);
        let entry = OrderEntry {
            escrow_addr: Address::from_bytes([2u8; 32]),
            side: OrderSide::Sell,
            sell_asset: 12345,
            sell_amount: 1000,
            buy_asset: 0,
            buy_amount: 2_000_000,
            price: 2_000_000,
            owner,
            created_round: Round(100),
            expire_round: Round(1000),
            status: EntryStatus::Active,
            filled_amount: 0,
            split_index: 0,
            parent_id: None,
            program: Vec::new(),
            params: EscrowParams::new(owner, 12345, 1000, 0, 2_000_000, 1000),
        };
        let resp: OrderResponse = entry.into();
        assert_eq!(resp.side, "sell");
        assert_eq!(resp.sell_amount, 1000);
        assert_eq!(resp.status, "active");
    }

    fn test_order_entry(
        side: OrderSide,
        sell_asset: u64,
        sell_amount: u64,
        buy_asset: u64,
        buy_amount: u64,
        escrow_byte: u8,
    ) -> OrderEntry {
        let owner = Address::from_bytes([escrow_byte; 32]);
        let (numerator, denominator) = match side {
            OrderSide::Sell => (buy_amount, sell_amount),
            OrderSide::Buy => (sell_amount, buy_amount),
        };
        let price = if denominator == 0 {
            0
        } else {
            ((u128::from(numerator) * 1_000_000u128) / u128::from(denominator))
                .min(u128::from(u64::MAX)) as u64
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
            created_round: Round(100),
            expire_round: Round(1000),
            status: EntryStatus::Active,
            filled_amount: 0,
            split_index: 0,
            parent_id: None,
            program,
            params: EscrowParams::new(owner, sell_asset, sell_amount, buy_asset, buy_amount, 1000),
        }
    }

    #[test]
    fn orderbook_route_candidate_quotes_sell_escrow_with_buy_ioc() {
        let db = opennodia_dex::DexDb::open_memory().unwrap();
        db.register_order(&test_order_entry(
            OrderSide::Sell,
            764789093,
            1,
            0,
            10_000,
            0x11,
        ))
        .unwrap();
        let book = DexOrderbookSource::new(&db);

        let candidate = orderbook_route_candidate(
            &book,
            OrderSide::Buy,
            OrderbookRouteContext {
                asset_in: 0,
                asset_out: 764789093,
                amount_in: 10_000,
                slippage_bps: 50,
                depth: 10,
                source_round: 100,
                writes_enabled: true,
            },
        )
        .unwrap()
        .unwrap();

        assert_eq!(candidate.execution, "native_orderbook_ioc_buy");
        assert_eq!(candidate.input_consumed, 10_000);
        assert_eq!(candidate.amount_out, 1);
        assert_eq!(candidate.remaining_input, 0);
        assert!(orderbook_route_candidate(
            &book,
            OrderSide::Sell,
            OrderbookRouteContext {
                asset_in: 0,
                asset_out: 764789093,
                amount_in: 10_000,
                slippage_bps: 50,
                depth: 10,
                source_round: 100,
                writes_enabled: true,
            },
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn orderbook_route_candidate_quotes_buy_escrow_with_sell_ioc() {
        let db = opennodia_dex::DexDb::open_memory().unwrap();
        db.register_order(&test_order_entry(
            OrderSide::Buy,
            0,
            4_500,
            764766481,
            1_000_000,
            0x22,
        ))
        .unwrap();
        let book = DexOrderbookSource::new(&db);

        let candidate = orderbook_route_candidate(
            &book,
            OrderSide::Sell,
            OrderbookRouteContext {
                asset_in: 764766481,
                asset_out: 0,
                amount_in: 1_000_000,
                slippage_bps: 50,
                depth: 10,
                source_round: 100,
                writes_enabled: true,
            },
        )
        .unwrap()
        .unwrap();

        assert_eq!(candidate.execution, "native_orderbook_ioc_sell");
        assert_eq!(candidate.input_consumed, 1_000_000);
        assert_eq!(candidate.amount_out, 4_500);
        assert_eq!(candidate.remaining_input, 0);
    }

    fn amm_candidate(input_consumed: u64, amount_out: u64) -> RouteCandidateResponse {
        RouteCandidateResponse {
            source: "native_amm".into(),
            source_label: "OpenNodia AMM".into(),
            execution: "native_amm_swap".into(),
            pool_id: Some("0:123".into()),
            app_id: Some(99),
            app_address: None,
            input_consumed,
            remaining_input: 0,
            amount_out,
            minimum_out: amount_out,
            fee_bps: 30,
            fee_amount_estimate: 3,
            price_impact_bps: 12,
            source_round: 100,
            executable: true,
            virtual_orderbook: true,
            note: "test".into(),
        }
    }

    #[test]
    fn synthetic_ask_uses_output_per_input_price() {
        let level = synthetic_level_from_candidate(amm_candidate(100, 250), true).unwrap();
        assert_eq!(level.amount, 100);
        assert_eq!(level.price, 2_500_000);
        assert_eq!(level.source, "native_amm");
    }

    #[test]
    fn synthetic_bid_uses_input_per_output_price() {
        let level = synthetic_level_from_candidate(amm_candidate(250, 100), false).unwrap();
        assert_eq!(level.amount, 100);
        assert_eq!(level.price, 2_500_000);
        assert_eq!(level.fee_bps, 30);
    }

    #[test]
    fn order_link_payload_preserves_order_fields() {
        let owner = Address::from_bytes([1u8; 32]);
        let escrow = Address::from_bytes([2u8; 32]);
        let entry = OrderEntry {
            escrow_addr: escrow,
            side: OrderSide::Buy,
            sell_asset: 0,
            sell_amount: 2_000_000,
            buy_asset: 12345,
            buy_amount: 1000,
            price: 2_000_000,
            owner,
            created_round: Round(100),
            expire_round: Round(1000),
            status: EntryStatus::Active,
            filled_amount: 0,
            split_index: 0,
            parent_id: None,
            program: Vec::new(),
            params: EscrowParams::new(owner, 0, 2_000_000, 12345, 1000, 1000),
        };

        let link = order_link_response_from_payload(order_link_payload_from_entry(&entry)).unwrap();
        assert!(link.url.starts_with("/#/dex/order/"));

        let decoded = decode_order_link(&link.payload).unwrap();
        assert_eq!(decoded.side, OrderSide::Buy);
        assert_eq!(decoded.owner_address(), owner);
        assert_eq!(decoded.escrow_address(), escrow);
        assert_eq!(decoded.sell_asset, 0);
        assert_eq!(decoded.buy_asset, 12345);
        assert_eq!(decoded.expire_round, 1000);
    }

    #[test]
    fn orderbook_bid_quote_consumes_best_levels_first() {
        let bids = vec![
            opennodia_dex::types::PriceLevel {
                price: 2_000_000,
                amount: 100,
                total: 100,
                order_count: 1,
            },
            opennodia_dex::types::PriceLevel {
                price: 1_500_000,
                amount: 100,
                total: 200,
                order_count: 1,
            },
        ];
        let (amount_out, input_consumed, price_impact_bps) =
            quote_orderbook_bids(&bids, 150).unwrap();
        assert_eq!(input_consumed, 150);
        assert_eq!(amount_out, 275);
        assert!(price_impact_bps > 0);
    }

    #[test]
    fn orderbook_bid_quote_reports_partial_liquidity() {
        let bids = vec![opennodia_dex::types::PriceLevel {
            price: 2_000_000,
            amount: 100,
            total: 100,
            order_count: 1,
        }];
        let (amount_out, input_consumed, price_impact_bps) =
            quote_orderbook_bids(&bids, 150).unwrap();
        assert_eq!(input_consumed, 100);
        assert_eq!(amount_out, 200);
        assert_eq!(price_impact_bps, 0);
    }
}
