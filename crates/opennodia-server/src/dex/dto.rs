use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use opennodia_dex::types::{OrderBookSnapshot, OrderEntry, PairStat, Trade};
use opennodia_swap::{OrderLinkPayload, OrderSide, OrderVerification};

use crate::tx_flow::TxDescription;

#[derive(Debug, Deserialize)]
pub struct PrepareCreateRequest {
    pub wallet_id: String,
    pub signer: String,
    pub side: String,
    pub sell_asset_id: u64,
    pub sell_amount: u64,
    pub buy_asset_id: u64,
    pub buy_amount: u64,
    #[serde(default = "default_expire_rounds")]
    pub expire_rounds: u64,
    #[serde(default = "default_split_count")]
    pub split_count: u32,
}

pub(super) fn default_expire_rounds() -> u64 {
    10_000
}

fn default_split_count() -> u32 {
    1
}

#[derive(Debug, Serialize)]
pub struct PrepareCreateResponse {
    pub intent_id: String,
    pub escrow_address: String,
    #[serde(default)]
    pub escrow_addresses: Vec<String>,
    pub split_count: u32,
    pub kind: String,
    pub owner_txs: Vec<TxDescription>,
    pub logicsig_txs: Vec<TxDescription>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitCreateRequest {
    pub wallet_id: String,
    pub pin: String,
    pub intent_id: String,
}

#[derive(Debug, Serialize)]
pub struct SubmitCreateResponse {
    pub tx_ids: Vec<String>,
    pub confirmed_round: u64,
    pub orders: Vec<OrderResponse>,
}

#[derive(Debug, Deserialize)]
pub struct PrepareFillRequest {
    pub wallet_id: String,
    pub filler: String,
    pub escrow_address: String,
}

#[derive(Debug, Serialize)]
pub struct PrepareFillResponse {
    pub intent_id: String,
    pub filler_tx: TxDescription,
    pub escrow_verified: bool,
    pub verification: VerificationResponse,
}

#[derive(Debug, Serialize)]
pub struct VerificationResponse {
    pub valid: bool,
    pub actual_balance: u64,
    pub expected_balance: u64,
    pub actual_asset_amount: u64,
    pub expected_asset_amount: u64,
    pub expired: bool,
    pub mismatch_reason: String,
}

impl From<OrderVerification> for VerificationResponse {
    fn from(v: OrderVerification) -> Self {
        Self {
            valid: v.valid,
            actual_balance: v.actual_balance,
            expected_balance: v.expected_balance,
            actual_asset_amount: v.actual_asset_amount,
            expected_asset_amount: v.expected_asset_amount,
            expired: v.expired,
            mismatch_reason: v.mismatch_reason,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SubmitFillRequest {
    pub wallet_id: String,
    pub pin: String,
    pub intent_id: String,
}

#[derive(Debug, Serialize)]
pub struct SubmitFillResponse {
    pub tx_id: String,
    pub confirmed_round: u64,
    pub recorded: bool,
    pub record_error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PrepareCancelRequest {
    pub wallet_id: String,
    pub escrow_address: String,
}

#[derive(Debug, Serialize)]
pub struct PrepareCancelResponse {
    pub intent_id: String,
    pub owner_auth_tx: TxDescription,
    pub escrow_txs: Vec<TxDescription>,
    pub recoverable_algo: u64,
    pub recoverable_asset: Option<(u64, u64)>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitCancelRequest {
    pub wallet_id: String,
    pub pin: String,
    pub intent_id: String,
}

#[derive(Debug, Serialize)]
pub struct SubmitCancelResponse {
    pub tx_id: String,
    pub confirmed_round: u64,
    pub recovered_amount: u64,
    pub recorded: bool,
    pub record_error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PrepareRouteRequest {
    pub wallet_id: String,
    pub filler: String,
    pub side: String,
    pub sell_asset_id: u64,
    pub sell_amount: u64,
    pub buy_asset_id: u64,
    pub buy_amount: u64,
    #[serde(default = "default_split_count")]
    pub split_count: u32,
    #[serde(default = "default_immediate_fill")]
    pub immediate_fill: bool,
    #[serde(default)]
    pub place_remaining: bool,
    #[serde(default = "default_expire_rounds")]
    pub expire_rounds: u64,
}

fn default_immediate_fill() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct PrepareRouteResponse {
    #[serde(default)]
    pub intent_id: String,
    pub decision: String,
    pub fills: Vec<FillPreview>,
    #[serde(default)]
    pub txs: Vec<TxDescription>,
    pub average_price: u64,
    pub total_cost: u64,
    pub total_received: u64,
    pub remaining: u64,
    pub new_orders_needed: u32,
    #[serde(default)]
    pub created_orders: Vec<OrderResponse>,
}

#[derive(Debug, Deserialize)]
pub struct RouteCandidatesRequest {
    #[serde(deserialize_with = "deserialize_u64")]
    pub asset_in: u64,
    #[serde(deserialize_with = "deserialize_u64")]
    pub asset_out: u64,
    #[serde(deserialize_with = "deserialize_u64")]
    pub amount_in: u64,
    #[serde(default = "default_slippage_bps")]
    pub slippage_bps: u16,
    #[serde(default = "default_depth")]
    pub depth: u32,
}

#[derive(Debug, Serialize)]
pub struct RouteCandidatesResponse {
    pub asset_in: u64,
    pub asset_out: u64,
    pub amount_in: u64,
    pub candidates: Vec<RouteCandidateResponse>,
    pub warnings: Vec<String>,
    pub source_round: u64,
    pub source: opennodia_node::DataSource,
}

#[derive(Debug, Serialize)]
pub struct RouteCandidateResponse {
    pub source: String,
    pub source_label: String,
    pub execution: String,
    pub pool_id: Option<String>,
    pub app_id: Option<u64>,
    pub app_address: Option<String>,
    pub input_consumed: u64,
    pub remaining_input: u64,
    pub amount_out: u64,
    pub minimum_out: u64,
    pub fee_bps: u16,
    pub fee_amount_estimate: u64,
    pub price_impact_bps: u64,
    pub source_round: u64,
    pub executable: bool,
    pub virtual_orderbook: bool,
    pub note: String,
}

#[derive(Debug, Serialize)]
pub struct FillPreview {
    pub escrow_address: String,
    pub amount: u64,
    pub price_micro: u64,
}

#[derive(Debug, Deserialize)]
pub struct SubmitRouteRequest {
    pub wallet_id: String,
    pub pin: String,
    pub intent_id: String,
}

#[derive(Debug, Serialize)]
pub struct SubmitRouteResponse {
    pub tx_id: String,
    pub tx_ids: Vec<String>,
    pub outcome: String,
    pub total_cost: u64,
    pub total_received: u64,
    pub failed_amount: u64,
    pub remaining: u64,
    pub fills: Vec<RouteFillResult>,
    #[serde(default)]
    pub created_orders: Vec<OrderResponse>,
}

#[derive(Debug, Serialize)]
pub struct RouteFillResult {
    pub escrow_address: String,
    pub status: String,
    pub tx_id: Option<String>,
    pub confirmed_round: Option<u64>,
    pub amount: u64,
    pub cost: u64,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OrderbookQuery {
    pub asset_a: u64,
    pub asset_b: u64,
    #[serde(default = "default_depth")]
    pub depth: u32,
}

fn default_depth() -> u32 {
    20
}

pub(super) fn default_slippage_bps() -> u16 {
    50
}

fn deserialize_u64<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum U64Value {
        Number(u64),
        String(String),
    }

    match U64Value::deserialize(deserializer)? {
        U64Value::Number(value) => Ok(value),
        U64Value::String(value) => value
            .trim()
            .parse::<u64>()
            .map_err(|error| D::Error::custom(format!("invalid u64 value: {error}"))),
    }
}

#[derive(Debug, Serialize)]
pub struct OrderbookResponse {
    pub pair: String,
    pub bids: Vec<PriceLevelResponse>,
    pub asks: Vec<PriceLevelResponse>,
    #[serde(default)]
    pub synthetic_bids: Vec<SyntheticPriceLevelResponse>,
    #[serde(default)]
    pub synthetic_asks: Vec<SyntheticPriceLevelResponse>,
    pub spread: u64,
    pub last_price: Option<u64>,
    pub last_update_round: u64,
    pub source: opennodia_node::DataSource,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PriceLevelResponse {
    pub price: u64,
    pub amount: u64,
    pub total: u64,
    pub order_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyntheticPriceLevelResponse {
    pub price: u64,
    pub amount: u64,
    pub total: u64,
    pub source: String,
    pub source_label: String,
    pub pool_id: Option<String>,
    pub app_id: Option<u64>,
    pub fee_bps: u16,
    pub fee_amount_estimate: u64,
    pub price_impact_bps: u64,
    pub executable: bool,
    pub source_round: u64,
    pub note: String,
}

impl From<opennodia_dex::types::PriceLevel> for PriceLevelResponse {
    fn from(p: opennodia_dex::types::PriceLevel) -> Self {
        Self {
            price: p.price,
            amount: p.amount,
            total: p.total,
            order_count: p.order_count,
        }
    }
}

impl OrderbookResponse {
    pub(super) fn from_snapshot(s: OrderBookSnapshot, source: opennodia_node::DataSource) -> Self {
        Self {
            pair: s.pair.display(),
            bids: s.bids.into_iter().map(Into::into).collect(),
            asks: s.asks.into_iter().map(Into::into).collect(),
            synthetic_bids: Vec::new(),
            synthetic_asks: Vec::new(),
            spread: s.spread,
            last_price: s.last_price,
            last_update_round: s.last_update_round.as_u64(),
            source,
            warnings: Vec::new(),
        }
    }
}

pub(super) fn assign_synthetic_totals(levels: &mut [SyntheticPriceLevelResponse]) {
    let mut total = 0u64;
    for level in levels {
        total = total.saturating_add(level.amount);
        level.total = total;
    }
}

#[derive(Debug, Deserialize)]
pub struct PairsQuery {
    #[serde(default = "default_recent_rounds")]
    pub recent_rounds: u64,
    #[serde(default = "default_pairs_limit")]
    pub limit: u32,
}

fn default_recent_rounds() -> u64 {
    26_000
}

fn default_pairs_limit() -> u32 {
    20
}

#[derive(Debug, Serialize)]
pub struct PairStatResponse {
    pub asset_a: u64,
    pub asset_b: u64,
    pub active_orders: u64,
    pub recent_trade_count: u64,
    pub recent_trade_volume: u64,
    pub last_price: Option<u64>,
    pub score: u64,
}

impl From<PairStat> for PairStatResponse {
    fn from(s: PairStat) -> Self {
        Self {
            asset_a: s.pair.asset_a,
            asset_b: s.pair.asset_b,
            active_orders: s.active_orders,
            recent_trade_count: s.recent_trade_count,
            recent_trade_volume: s.recent_trade_volume,
            last_price: s.last_price,
            score: s.score,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PairsResponse {
    pub pairs: Vec<PairStatResponse>,
    pub source_round: u64,
    pub source: opennodia_node::DataSource,
}

#[derive(Debug, Serialize)]
pub struct DexStatusResponse {
    #[serde(flatten)]
    pub validation: crate::dex_validation::DexValidationSnapshot,
    pub write_enabled: bool,
    pub active_orders: u64,
    pub last_reconciled_round: u64,
}

#[derive(Debug, Deserialize)]
pub struct MyOrdersQuery {
    pub wallet_id: String,
    #[serde(default = "default_status_filter")]
    pub status: String,
}

fn default_status_filter() -> String {
    "all".to_string()
}

#[derive(Debug, Serialize)]
pub struct MyOrdersResponse {
    pub orders: Vec<OrderResponse>,
}

#[derive(Debug, Serialize)]
pub struct OrderResponse {
    pub escrow_addr: String,
    pub side: String,
    pub sell_asset: u64,
    pub sell_amount: u64,
    pub buy_asset: u64,
    pub buy_amount: u64,
    pub price: u64,
    pub owner: String,
    pub created_round: u64,
    pub expire_round: u64,
    pub status: String,
    pub filled_amount: u64,
}

impl From<OrderEntry> for OrderResponse {
    fn from(o: OrderEntry) -> Self {
        Self {
            escrow_addr: format!("{}", o.escrow_addr),
            side: o.side.as_str().to_string(),
            sell_asset: o.sell_asset,
            sell_amount: o.sell_amount,
            buy_asset: o.buy_asset,
            buy_amount: o.buy_amount,
            price: o.price,
            owner: format!("{}", o.owner),
            created_round: o.created_round.as_u64(),
            expire_round: o.expire_round.as_u64(),
            status: o.status.as_str().to_string(),
            filled_amount: o.filled_amount,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct TradesQuery {
    pub pair: Option<String>,
    pub wallet_id: Option<String>,
    pub address: Option<String>,
    #[serde(default = "default_trade_limit")]
    pub limit: u32,
}

fn default_trade_limit() -> u32 {
    50
}

#[derive(Debug, Serialize)]
pub struct TradesResponse {
    pub trades: Vec<TradeResponse>,
}

#[derive(Debug, Serialize)]
pub struct TradeResponse {
    pub tx_id: String,
    pub pair: String,
    pub side: String,
    pub price: u64,
    pub amount: u64,
    pub buyer: String,
    pub seller: String,
    pub round: u64,
    pub timestamp: u64,
}

impl From<Trade> for TradeResponse {
    fn from(t: Trade) -> Self {
        Self::from_view(t, None)
    }
}

impl TradeResponse {
    pub(super) fn from_view(t: Trade, view_base_asset: Option<u64>) -> Self {
        let should_invert = view_base_asset
            .zip(t.base_asset)
            .is_some_and(|(view_base, trade_base)| view_base != trade_base);
        let price = if should_invert {
            opennodia_dex::types::invert_price(t.price)
        } else {
            t.price
        };
        let amount = if should_invert {
            ((u128::from(t.amount) * u128::from(t.price)) / 1_000_000u128).min(u128::from(u64::MAX))
                as u64
        } else {
            t.amount
        };
        let side = if should_invert {
            match t.side {
                OrderSide::Buy => OrderSide::Sell,
                OrderSide::Sell => OrderSide::Buy,
            }
        } else {
            t.side
        };
        Self {
            tx_id: t.tx_id,
            pair: t.pair.display(),
            side: side.as_str().to_string(),
            price,
            amount,
            buyer: format!("{}", t.buyer),
            seller: format!("{}", t.seller),
            round: t.round.as_u64(),
            timestamp: t.timestamp,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct OrderDetailResponse {
    pub order: Option<OrderResponse>,
    pub verification: Option<VerificationResponse>,
    pub source_round: u64,
    pub source: opennodia_node::DataSource,
}

#[derive(Debug, Serialize)]
pub struct OrderLinkPayloadResponse {
    pub version: u8,
    pub side: String,
    pub sell_asset: u64,
    pub sell_amount: u64,
    pub buy_asset: u64,
    pub buy_amount: u64,
    pub owner: String,
    pub escrow: String,
    pub expire_round: u64,
}

impl From<&OrderLinkPayload> for OrderLinkPayloadResponse {
    fn from(payload: &OrderLinkPayload) -> Self {
        Self {
            version: payload.version,
            side: payload.side.as_str().to_string(),
            sell_asset: payload.sell_asset,
            sell_amount: payload.sell_amount,
            buy_asset: payload.buy_asset,
            buy_amount: payload.buy_amount,
            owner: payload.owner_address().to_string(),
            escrow: payload.escrow_address().to_string(),
            expire_round: payload.expire_round,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct OrderLinkGenerateResponse {
    pub payload: String,
    pub url: String,
    pub decoded: OrderLinkPayloadResponse,
}

#[derive(Debug, Serialize)]
pub struct OrderLinkResolutionResponse {
    pub status: String,
    pub tx_id: Option<String>,
    pub round: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct OrderLinkDetailResponse {
    pub payload: String,
    pub url: String,
    pub decoded: OrderLinkPayloadResponse,
    pub payload_valid: bool,
    pub canonical_escrow_match: bool,
    pub canonical_escrow_address: Option<String>,
    pub status: String,
    pub order: Option<OrderResponse>,
    pub resolution: Option<OrderLinkResolutionResponse>,
    pub verification: Option<VerificationResponse>,
    pub source_round: u64,
    pub source: opennodia_node::DataSource,
    pub error: Option<String>,
}
