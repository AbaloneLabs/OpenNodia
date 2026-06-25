use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::tx_flow::{TxDescription, WalletTxGroup};

#[derive(Debug, Clone)]
pub(crate) enum RouterIntentAction {
    Delegated {
        source_type: String,
        source_id: String,
        quote_id: String,
        route_hash: String,
        submit: RouterSubmitDelegate,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum RouterSubmitDelegate {
    Orderbook {
        intent_id: String,
    },
    NativePool {
        intent_id: String,
        fields: crate::lp::PoolSwapFields,
    },
    ExternalPool {
        intent_id: String,
        fields: crate::external_liquidity::ExternalSwapFields,
    },
    NativeSplit {
        group: WalletTxGroup,
        tx_hash: String,
        legs: Vec<PreparedNativeSplitLeg>,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedNativeSplitLeg {
    pub app_id: u64,
    pub pool_id: String,
    pub pool_before: opennodia_amm::PoolState,
    pub quote: opennodia_amm::SwapQuote,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RouterQuoteRequest {
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
    /// Optional source pin: `best`, `orderbook`, `native_pool`, `external_pool`,
    /// `external_tinyman`, `external_pact`, or an exact `source_id`.
    pub source: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RouterPrepareRequest {
    pub wallet_id: String,
    pub trader: String,
    pub quote_id: String,
    pub route_hash: String,
    #[serde(flatten)]
    pub quote: RouterQuoteRequest,
    #[serde(
        default = "default_expire_rounds",
        deserialize_with = "deserialize_u64"
    )]
    pub expire_rounds: u64,
}

#[derive(Debug, Deserialize)]
pub struct RouterSubmitRequest {
    pub wallet_id: String,
    pub pin: String,
    pub intent_id: String,
    pub quote_id: String,
    pub route_hash: String,
}

#[derive(Debug, Serialize)]
pub struct RouterQuoteResponse {
    pub quote_id: String,
    pub network: String,
    pub asset_in: u64,
    pub asset_out: u64,
    pub amount_in: u64,
    pub slippage_bps: u16,
    pub source_round: u64,
    pub expires_after_round: u64,
    pub comparison_basis: String,
    pub selected: Option<UnifiedRouteQuote>,
    pub second_best: Option<UnifiedRouteQuote>,
    pub candidates: Vec<UnifiedRouteQuote>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnifiedRouteQuote {
    pub route_hash: String,
    pub source_type: String,
    pub source_id: String,
    pub source_label: String,
    pub execution: String,
    pub canonical_id: String,
    pub pool_id: Option<String>,
    pub app_id: Option<u64>,
    pub app_address: Option<String>,
    pub asset_in: u64,
    pub asset_out: u64,
    pub amount_in: u64,
    pub input_consumed: u64,
    pub remaining_input: u64,
    pub amount_out: u64,
    pub minimum_out: u64,
    pub lp_fee_bps: u16,
    pub lp_fee_amount: u64,
    pub protocol_fee_bps: u16,
    pub protocol_fee_amount: u64,
    pub network_fee_microalgo: u64,
    pub price_impact_bps: u64,
    pub source_round: u64,
    pub expires_after_round: u64,
    pub executable: bool,
    pub virtual_orderbook: bool,
    #[serde(default)]
    pub split_legs: Vec<UnifiedRouteLegQuote>,
    pub selection_rank: Option<u32>,
    pub selection_reason: Option<String>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnifiedRouteLegQuote {
    pub source_type: String,
    pub source_id: String,
    pub source_label: String,
    pub canonical_id: String,
    pub pool_id: Option<String>,
    pub app_id: Option<u64>,
    pub asset_in: u64,
    pub asset_out: u64,
    pub amount_in: u64,
    pub amount_out: u64,
    pub minimum_out: u64,
    pub lp_fee_bps: u16,
    pub lp_fee_amount: u64,
    pub network_fee_microalgo: u64,
    pub source_round: u64,
}

#[derive(Debug, Serialize)]
pub struct RouterPrepareResponse {
    pub intent_id: String,
    pub quote_id: String,
    pub route_hash: String,
    pub source_type: String,
    pub source_id: String,
    pub tx_hash: Option<String>,
    pub txs: Vec<TxDescription>,
    pub preview: Value,
    pub selected: UnifiedRouteQuote,
}

#[derive(Debug, Serialize)]
pub struct RouterSubmitResponse {
    pub quote_id: String,
    pub route_hash: String,
    pub source_type: String,
    pub source_id: String,
    pub tx_id: Option<String>,
    pub tx_ids: Vec<String>,
    pub confirmed_round: Option<u64>,
    pub outcome: String,
    pub result: Value,
}

fn default_slippage_bps() -> u16 {
    50
}

fn default_depth() -> u32 {
    20
}

fn default_expire_rounds() -> u64 {
    1_000
}

fn deserialize_u64<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
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
