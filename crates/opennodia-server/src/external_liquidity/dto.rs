use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

use opennodia_amm::{AddLiquidityQuote, RemoveLiquidityQuote, SwapQuote};
use opennodia_node::DataSource;

use crate::tx_flow::{self, WalletTxGroup};

const DEFAULT_EXTERNAL_SWAP_EXPIRE_ROUNDS: u64 = 1_000;

#[derive(Debug, Deserialize)]
pub struct ExternalPoolListQuery {
    pub asset_a: Option<u64>,
    pub asset_b: Option<u64>,
    pub source: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ExternalQuoteRequest {
    pub source: String,
    pub pool_id: String,
    #[serde(deserialize_with = "deserialize_u64")]
    pub asset_in: u64,
    #[serde(deserialize_with = "deserialize_u64")]
    pub amount_in: u64,
    #[serde(default = "default_slippage_bps")]
    pub slippage_bps: u16,
}

#[derive(Debug, Deserialize)]
pub struct ExternalPositionQuery {
    pub address: String,
    pub asset_a: Option<u64>,
    pub asset_b: Option<u64>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExternalSwapFields {
    pub source: String,
    pub pool_id: String,
    pub trader: String,
    #[serde(deserialize_with = "deserialize_u64")]
    pub asset_in: u64,
    #[serde(deserialize_with = "deserialize_u64")]
    pub amount_in: u64,
    #[serde(default = "default_slippage_bps")]
    pub slippage_bps: u16,
    #[serde(
        default = "default_external_swap_expire_rounds",
        deserialize_with = "deserialize_u64"
    )]
    pub expire_rounds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExternalSwapPrepareRequest {
    pub wallet_id: String,
    #[serde(flatten)]
    pub fields: ExternalSwapFields,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExternalSwapSubmitRequest {
    pub wallet_id: String,
    pub pin: String,
    pub intent_id: String,
    #[serde(flatten)]
    pub fields: ExternalSwapFields,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExternalAddLiquidityFields {
    pub source: String,
    pub pool_id: String,
    pub provider: String,
    #[serde(deserialize_with = "deserialize_u64")]
    pub amount_0: u64,
    #[serde(deserialize_with = "deserialize_u64")]
    pub amount_1: u64,
    #[serde(default = "default_slippage_bps")]
    pub slippage_bps: u16,
    #[serde(
        default = "default_external_swap_expire_rounds",
        deserialize_with = "deserialize_u64"
    )]
    pub expire_rounds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExternalAddLiquidityPrepareRequest {
    pub wallet_id: String,
    #[serde(flatten)]
    pub fields: ExternalAddLiquidityFields,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExternalAddLiquiditySubmitRequest {
    pub wallet_id: String,
    pub pin: String,
    pub intent_id: String,
    #[serde(flatten)]
    pub fields: ExternalAddLiquidityFields,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExternalRemoveLiquidityFields {
    pub source: String,
    pub pool_id: String,
    pub provider: String,
    #[serde(deserialize_with = "deserialize_u64")]
    pub burn_lp: u64,
    #[serde(default = "default_slippage_bps")]
    pub slippage_bps: u16,
    #[serde(
        default = "default_external_swap_expire_rounds",
        deserialize_with = "deserialize_u64"
    )]
    pub expire_rounds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExternalRemoveLiquidityPrepareRequest {
    pub wallet_id: String,
    #[serde(flatten)]
    pub fields: ExternalRemoveLiquidityFields,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExternalRemoveLiquiditySubmitRequest {
    pub wallet_id: String,
    pub pin: String,
    pub intent_id: String,
    #[serde(flatten)]
    pub fields: ExternalRemoveLiquidityFields,
}

#[derive(Debug, Serialize)]
pub struct ExternalLiquidityStatusResponse {
    pub network: String,
    pub sources: Vec<ExternalSourceStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExternalSourceStatus {
    pub source: String,
    pub label: String,
    pub quote_supported: bool,
    pub swap_supported: bool,
    pub liquidity_supported: bool,
    pub position_supported: bool,
    pub status: String,
    pub validator_app_id: Option<u64>,
    pub factory_app_id: Option<u64>,
    pub folks_lending_pool_adapter_app_id: Option<u64>,
    pub note: String,
}

#[derive(Debug, Serialize)]
pub struct ExternalPoolListResponse {
    pub pools: Vec<ExternalPoolResponse>,
    pub source: DataSource,
    pub source_round: u64,
    pub discovery_note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ExternalPositionListResponse {
    pub address: String,
    pub positions: Vec<ExternalLpPositionResponse>,
    pub source: DataSource,
    pub source_round: u64,
    pub discovery_note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ExternalQuoteResponse {
    pub pool: ExternalPoolResponse,
    pub quote: SwapQuote,
    pub source: DataSource,
}

#[derive(Debug, Serialize)]
pub struct ExternalSwapPrepareResponse {
    pub intent_id: String,
    pub tx_hash: String,
    pub txs: Vec<tx_flow::TxDescription>,
    pub preview: ExternalSwapPreview,
}

#[derive(Debug, Serialize)]
pub struct ExternalSwapSubmitResponse {
    pub txid: String,
    pub confirmed_round: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmed_amount_out: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_out_balance_after: Option<u64>,
    pub pool: ExternalPoolResponse,
    pub quote: SwapQuote,
}

#[derive(Debug, Serialize)]
pub struct ExternalAddLiquidityPrepareResponse {
    pub intent_id: String,
    pub tx_hash: String,
    pub txs: Vec<tx_flow::TxDescription>,
    pub preview: ExternalAddLiquidityPreview,
}

#[derive(Debug, Serialize)]
pub struct ExternalAddLiquiditySubmitResponse {
    pub txid: String,
    pub confirmed_round: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minted_lp: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lp_balance_after: Option<u64>,
    pub pool: ExternalPoolResponse,
    pub quote: AddLiquidityQuote,
}

#[derive(Debug, Serialize)]
pub struct ExternalRemoveLiquidityPrepareResponse {
    pub intent_id: String,
    pub tx_hash: String,
    pub txs: Vec<tx_flow::TxDescription>,
    pub preview: ExternalRemoveLiquidityPreview,
}

#[derive(Debug, Serialize)]
pub struct ExternalRemoveLiquiditySubmitResponse {
    pub txid: String,
    pub confirmed_round: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_0: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_1: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lp_balance_after: Option<u64>,
    pub pool: ExternalPoolResponse,
    pub quote: RemoveLiquidityQuote,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExternalPoolResponse {
    pub pool_id: String,
    pub source: String,
    pub app_id: u64,
    pub app_address: String,
    pub lp_asset_id: u64,
    pub asset_0: u64,
    pub asset_1: u64,
    pub fee_bps: u16,
    pub protocol_fee_bps: Option<u16>,
    pub protocol_version: String,
    pub reserve_0: u64,
    pub reserve_1: u64,
    pub total_lp_supply: u64,
    pub source_round: u64,
    pub quote_supported: bool,
    pub swap_supported: bool,
    pub adapter_swap_supported: bool,
    pub position_supported: bool,
    pub tradable: bool,
    pub folks_backed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folks: Option<FolksBackedInfo>,
    pub status: String,
    pub status_note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FolksBackedInfo {
    pub source: String,
    pub adapter_app_id: Option<u64>,
    pub pool_0_app_id: u64,
    pub pool_1_app_id: u64,
    pub underlying_0: u64,
    pub underlying_1: u64,
    pub f_asset_0: u64,
    pub f_asset_1: u64,
    pub deposit_interest_rate_0: u64,
    pub deposit_interest_rate_1: u64,
    pub deposit_interest_index_0: u64,
    pub deposit_interest_index_1: u64,
    pub redeem_available_0: u64,
    pub redeem_available_1: u64,
    pub f_asset_outstanding_0: u64,
    pub f_asset_outstanding_1: u64,
    pub total_deposit_0: u64,
    pub total_deposit_1: u64,
    pub total_borrowed_0: u64,
    pub total_borrowed_1: u64,
    pub utilization_bps_0: u64,
    pub utilization_bps_1: u64,
    pub utilization_available: bool,
    pub utilization_note: String,
    pub risk_note: String,
}

#[derive(Debug, Serialize)]
pub struct ExternalLpPositionResponse {
    pub pool: ExternalPoolResponse,
    pub lp_asset_id: u64,
    pub lp_balance: u64,
    pub pool_share_ppm: u64,
    pub underlying_0: u64,
    pub underlying_1: u64,
    pub position_source: String,
    pub add_supported: bool,
    pub remove_supported: bool,
    pub reward_apr_included: bool,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExternalSwapPreview {
    pub source: String,
    pub trader: String,
    pub pool_id: String,
    pub asset_in: u64,
    pub asset_out: u64,
    pub amount_in: u64,
    pub amount_out: u64,
    pub minimum_out: u64,
    pub fee_bps: u16,
    pub fee_amount_estimate: u64,
    pub price_impact_bps: u64,
    pub source_round: u64,
    pub deadline_round: u64,
    pub total_fee: u64,
    pub atomic_group_size: usize,
    pub verification_note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExternalAddLiquidityPreview {
    pub source: String,
    pub provider: String,
    pub app_id: u64,
    pub pool_id: String,
    pub amount_0: u64,
    pub amount_1: u64,
    pub minted_lp: u64,
    pub minimum_lp: u64,
    pub deadline_round: u64,
    pub total_fee: u64,
    pub atomic_group_size: usize,
    pub foreign_assets: Vec<u64>,
    pub verification_note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExternalRemoveLiquidityPreview {
    pub source: String,
    pub provider: String,
    pub app_id: u64,
    pub pool_id: String,
    pub burn_lp: u64,
    pub amount_0: u64,
    pub amount_1: u64,
    pub minimum_0: u64,
    pub minimum_1: u64,
    pub deadline_round: u64,
    pub total_fee: u64,
    pub atomic_group_size: usize,
    pub foreign_assets: Vec<u64>,
    pub verification_note: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ExternalRouteQuoteCandidate {
    pub(crate) pool: ExternalPoolResponse,
    pub(crate) quote: SwapQuote,
}

#[derive(Debug, Clone)]
pub(crate) struct ExternalRouteQuoteCandidates {
    pub(crate) candidates: Vec<ExternalRouteQuoteCandidate>,
    pub(crate) source: DataSource,
    pub(crate) source_round: u64,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) enum ExternalLiquidityIntentAction {
    Swap {
        group: WalletTxGroup,
        pool_before: ExternalPoolResponse,
        quote: SwapQuote,
        slippage_bps: u16,
    },
    Add {
        group: WalletTxGroup,
        pool_before: ExternalPoolResponse,
        quote: AddLiquidityQuote,
        slippage_bps: u16,
    },
    Remove {
        group: WalletTxGroup,
        pool_before: ExternalPoolResponse,
        quote: RemoveLiquidityQuote,
        slippage_bps: u16,
    },
}

fn default_slippage_bps() -> u16 {
    50
}

fn default_external_swap_expire_rounds() -> u64 {
    DEFAULT_EXTERNAL_SWAP_EXPIRE_ROUNDS
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
