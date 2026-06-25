use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

use opennodia_amm::transactions::POOL_SETUP_BASE_FUNDING_MICROALGO;
use opennodia_amm::{AddLiquidityQuote, RemoveLiquidityQuote, SwapQuote};
use opennodia_node::DataSource;

use crate::tx_flow;

#[derive(Debug, Deserialize)]
pub struct PoolListQuery {
    pub asset_a: Option<u64>,
    pub asset_b: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct PositionQuery {
    pub address: String,
}

#[derive(Debug, Deserialize)]
pub struct QuoteRequest {
    #[serde(deserialize_with = "deserialize_u64")]
    pub app_id: u64,
    #[serde(deserialize_with = "deserialize_u64")]
    pub asset_in: u64,
    #[serde(deserialize_with = "deserialize_u64")]
    pub amount_in: u64,
    #[serde(default = "default_slippage_bps")]
    pub slippage_bps: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegistryCreateFields {
    pub creator: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegistryCreatePrepareRequest {
    pub wallet_id: String,
    #[serde(flatten)]
    pub fields: RegistryCreateFields,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegistryCreateSubmitRequest {
    pub wallet_id: String,
    pub pin: String,
    pub intent_id: String,
    #[serde(flatten)]
    pub fields: RegistryCreateFields,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PoolCreateFields {
    pub creator: String,
    #[serde(deserialize_with = "deserialize_u64")]
    pub asset_a: u64,
    #[serde(deserialize_with = "deserialize_u64")]
    pub asset_b: u64,
    pub fee_bps: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PoolCreatePrepareRequest {
    pub wallet_id: String,
    #[serde(flatten)]
    pub fields: PoolCreateFields,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PoolCreateSubmitRequest {
    pub wallet_id: String,
    pub pin: String,
    pub intent_id: String,
    #[serde(flatten)]
    pub fields: PoolCreateFields,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PoolSetupFields {
    pub creator: String,
    #[serde(deserialize_with = "deserialize_u64")]
    pub app_id: u64,
    #[serde(default = "default_pool_setup_funding")]
    pub funding_microalgo: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PoolSetupPrepareRequest {
    pub wallet_id: String,
    #[serde(flatten)]
    pub fields: PoolSetupFields,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PoolSetupSubmitRequest {
    pub wallet_id: String,
    pub pin: String,
    pub intent_id: String,
    #[serde(flatten)]
    pub fields: PoolSetupFields,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PoolBootstrapFields {
    pub provider: String,
    #[serde(deserialize_with = "deserialize_u64")]
    pub app_id: u64,
    #[serde(deserialize_with = "deserialize_u64")]
    pub amount_0: u64,
    #[serde(deserialize_with = "deserialize_u64")]
    pub amount_1: u64,
    #[serde(default = "default_slippage_bps")]
    pub slippage_bps: u16,
    #[serde(
        default = "default_expire_rounds",
        deserialize_with = "deserialize_u64"
    )]
    pub expire_rounds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PoolBootstrapPrepareRequest {
    pub wallet_id: String,
    #[serde(flatten)]
    pub fields: PoolBootstrapFields,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PoolBootstrapSubmitRequest {
    pub wallet_id: String,
    pub pin: String,
    pub intent_id: String,
    #[serde(flatten)]
    pub fields: PoolBootstrapFields,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PoolAddFields {
    pub provider: String,
    #[serde(deserialize_with = "deserialize_u64")]
    pub app_id: u64,
    #[serde(deserialize_with = "deserialize_u64")]
    pub desired_0: u64,
    #[serde(deserialize_with = "deserialize_u64")]
    pub desired_1: u64,
    #[serde(default = "default_slippage_bps")]
    pub slippage_bps: u16,
    #[serde(
        default = "default_expire_rounds",
        deserialize_with = "deserialize_u64"
    )]
    pub expire_rounds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PoolAddPrepareRequest {
    pub wallet_id: String,
    #[serde(flatten)]
    pub fields: PoolAddFields,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PoolAddSubmitRequest {
    pub wallet_id: String,
    pub pin: String,
    pub intent_id: String,
    #[serde(flatten)]
    pub fields: PoolAddFields,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PoolRemoveFields {
    pub provider: String,
    #[serde(deserialize_with = "deserialize_u64")]
    pub app_id: u64,
    #[serde(deserialize_with = "deserialize_u64")]
    pub burn_lp: u64,
    #[serde(default = "default_slippage_bps")]
    pub slippage_bps: u16,
    #[serde(
        default = "default_expire_rounds",
        deserialize_with = "deserialize_u64"
    )]
    pub expire_rounds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PoolRemovePrepareRequest {
    pub wallet_id: String,
    #[serde(flatten)]
    pub fields: PoolRemoveFields,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PoolRemoveSubmitRequest {
    pub wallet_id: String,
    pub pin: String,
    pub intent_id: String,
    #[serde(flatten)]
    pub fields: PoolRemoveFields,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PoolSwapFields {
    pub trader: String,
    #[serde(deserialize_with = "deserialize_u64")]
    pub app_id: u64,
    #[serde(deserialize_with = "deserialize_u64")]
    pub asset_in: u64,
    #[serde(deserialize_with = "deserialize_u64")]
    pub amount_in: u64,
    #[serde(default = "default_slippage_bps")]
    pub slippage_bps: u16,
    #[serde(
        default = "default_expire_rounds",
        deserialize_with = "deserialize_u64"
    )]
    pub expire_rounds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PoolSwapPrepareRequest {
    pub wallet_id: String,
    #[serde(flatten)]
    pub fields: PoolSwapFields,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PoolSwapSubmitRequest {
    pub wallet_id: String,
    pub pin: String,
    pub intent_id: String,
    #[serde(flatten)]
    pub fields: PoolSwapFields,
}

fn default_slippage_bps() -> u16 {
    50
}

fn default_expire_rounds() -> u64 {
    1_000
}

fn default_pool_setup_funding() -> u64 {
    POOL_SETUP_BASE_FUNDING_MICROALGO
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

#[derive(Debug, Serialize)]
pub struct LpStatusResponse {
    pub native_contract_version: u16,
    pub curve_id: u16,
    pub fee_tiers_bps: Vec<u16>,
    pub create_setup_enabled: bool,
    pub swap_liquidity_enabled: bool,
    pub mainnet_write_enabled_after_audit: bool,
    pub write_status_note: String,
    pub native_registry_app_id: Option<u64>,
    pub registry_required: bool,
}

#[derive(Debug, Serialize)]
pub struct PoolListResponse {
    pub pools: Vec<PoolResponse>,
    pub source: DataSource,
    pub source_round: u64,
    pub discovery_note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LpPositionsResponse {
    pub address: String,
    pub positions: Vec<LpPositionResponse>,
    pub source: DataSource,
    pub source_round: u64,
    pub discovery_note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LpPositionResponse {
    pub pool: PoolResponse,
    pub lp_asset_id: u64,
    pub lp_balance: u64,
    pub pool_share_ppm: u64,
    pub underlying_0: u64,
    pub underlying_1: u64,
    pub cost_basis_available: bool,
    pub pnl_available: bool,
    pub note: String,
}

#[derive(Debug, Serialize)]
pub struct PoolDetailResponse {
    pub pool: PoolResponse,
    pub app_address: String,
    pub source: DataSource,
}

#[derive(Debug, Serialize)]
pub struct PoolQuoteResponse {
    pub pool: PoolResponse,
    pub quote: opennodia_amm::SwapQuote,
    pub source: DataSource,
}

#[derive(Debug, Serialize)]
pub struct RegistryCreatePrepareResponse {
    pub intent_id: String,
    pub tx_hash: String,
    pub tx_bytes: String,
    pub preview: RegistryCreatePreview,
}

#[derive(Debug, Serialize)]
pub struct RegistryCreateSubmitResponse {
    pub txid: String,
    pub confirmed_round: u64,
    pub app_id: u64,
    pub app_address: String,
}

#[derive(Debug, Serialize)]
pub struct PoolCreatePrepareResponse {
    pub intent_id: String,
    pub tx_hash: String,
    pub tx_bytes: String,
    pub txs: Vec<TxBytesResponse>,
    pub preview: PoolCreatePreview,
}

#[derive(Debug, Serialize)]
pub struct PoolCreateSubmitResponse {
    pub txid: String,
    pub confirmed_round: u64,
    pub app_id: u64,
    pub app_address: String,
    pub pool_id: String,
    pub asset_0: u64,
    pub asset_1: u64,
    pub fee_bps: u16,
}

#[derive(Debug, Serialize)]
pub struct PoolSetupPrepareResponse {
    pub intent_id: String,
    pub tx_hash: String,
    pub txs: Vec<TxBytesResponse>,
    pub preview: PoolSetupPreview,
}

#[derive(Debug, Serialize)]
pub struct PoolSetupSubmitResponse {
    pub txid: String,
    pub confirmed_round: u64,
    pub pool: PoolResponse,
}

#[derive(Debug, Serialize)]
pub struct PoolBootstrapPrepareResponse {
    pub intent_id: String,
    pub tx_hash: String,
    pub txs: Vec<TxBytesResponse>,
    pub preview: PoolAddLiquidityPreview,
}

#[derive(Debug, Serialize)]
pub struct PoolBootstrapSubmitResponse {
    pub txid: String,
    pub confirmed_round: u64,
    pub pool: PoolResponse,
    pub quote: AddLiquidityQuote,
}

#[derive(Debug, Serialize)]
pub struct PoolAddPrepareResponse {
    pub intent_id: String,
    pub tx_hash: String,
    pub txs: Vec<TxBytesResponse>,
    pub preview: PoolAddLiquidityPreview,
}

#[derive(Debug, Serialize)]
pub struct PoolAddSubmitResponse {
    pub txid: String,
    pub confirmed_round: u64,
    pub pool: PoolResponse,
    pub quote: AddLiquidityQuote,
}

#[derive(Debug, Serialize)]
pub struct PoolRemovePrepareResponse {
    pub intent_id: String,
    pub tx_hash: String,
    pub txs: Vec<TxBytesResponse>,
    pub preview: PoolRemoveLiquidityPreview,
}

#[derive(Debug, Serialize)]
pub struct PoolRemoveSubmitResponse {
    pub txid: String,
    pub confirmed_round: u64,
    pub pool: PoolResponse,
    pub quote: RemoveLiquidityQuote,
}

#[derive(Debug, Serialize)]
pub struct PoolSwapPrepareResponse {
    pub intent_id: String,
    pub tx_hash: String,
    pub txs: Vec<TxBytesResponse>,
    pub preview: PoolSwapPreview,
}

#[derive(Debug, Serialize)]
pub struct PoolSwapSubmitResponse {
    pub txid: String,
    pub confirmed_round: u64,
    pub pool: PoolResponse,
    pub quote: SwapQuote,
}

pub type TxBytesResponse = tx_flow::TxDescription;

#[derive(Debug, Serialize)]
pub struct RegistryCreatePreview {
    pub creator: String,
    pub registry_version: u16,
    pub pool_approval_hash: String,
    pub pool_clear_hash: String,
    pub app_create_fee: u64,
}

#[derive(Debug, Serialize)]
pub struct PoolCreatePreview {
    pub creator: String,
    pub asset_0: u64,
    pub asset_1: u64,
    pub fee_bps: u16,
    pub pool_id: String,
    pub app_create_fee: u64,
    pub registered_on_create: bool,
    pub registry_app_id: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct PoolSetupPreview {
    pub creator: String,
    pub app_id: u64,
    pub app_address: String,
    pub pool_id: String,
    pub funding_microalgo: u64,
    pub funding_algo: String,
    pub setup_fee: u64,
    pub foreign_assets: Vec<u64>,
}

#[derive(Debug, Serialize)]
pub struct PoolAddLiquidityPreview {
    pub provider: String,
    pub app_id: u64,
    pub pool_id: String,
    pub operation: String,
    pub amount_0: u64,
    pub amount_1: u64,
    pub minted_lp: u64,
    pub minimum_lp: u64,
    pub deadline_round: u64,
    pub total_fee: u64,
    pub atomic_group_size: usize,
    pub foreign_assets: Vec<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bootstrap: Option<PoolBootstrapSafetyPreview>,
}

#[derive(Debug, Serialize)]
pub struct PoolBootstrapSafetyPreview {
    pub initial_price_numerator: u64,
    pub initial_price_denominator: u64,
    pub initial_price_display: String,
    pub price_impact_bps: u64,
    pub provider_current_microalgo: u64,
    pub provider_min_balance_microalgo: u64,
    pub provider_available_microalgo: u64,
    pub network_fee_microalgo: u64,
    pub will_be_tradable: bool,
    pub note: String,
}

#[derive(Debug, Serialize)]
pub struct PoolRemoveLiquidityPreview {
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
    pub foreign_assets: Vec<u64>,
}

#[derive(Debug, Serialize)]
pub struct PoolSwapPreview {
    pub trader: String,
    pub app_id: u64,
    pub pool_id: String,
    pub asset_in: u64,
    pub asset_out: u64,
    pub amount_in: u64,
    pub amount_out: u64,
    pub minimum_out: u64,
    pub fee_bps: u16,
    pub price_impact_bps: u64,
    pub deadline_round: u64,
    pub total_fee: u64,
    pub foreign_assets: Vec<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PoolResponse {
    pub pool_id: String,
    pub source: String,
    pub app_id: u64,
    pub app_address: String,
    pub lp_asset_id: u64,
    pub asset_0: u64,
    pub asset_1: u64,
    pub fee_bps: u16,
    pub curve_id: u16,
    pub contract_version: u16,
    pub reserve_0: u64,
    pub reserve_1: u64,
    pub total_lp_supply: u64,
    pub source_round: u64,
    pub lifecycle: String,
    pub tradable: bool,
    pub active_liquidity: bool,
    pub setup_required: bool,
    pub bootstrap_required: bool,
    pub status_note: String,
}

#[derive(Debug, Clone)]
pub(crate) struct NativeRouteQuoteCandidate {
    pub(crate) pool: PoolResponse,
    pub(crate) quote: SwapQuote,
}

#[derive(Debug, Clone)]
pub(crate) struct NativeRouteQuoteCandidates {
    pub(crate) candidates: Vec<NativeRouteQuoteCandidate>,
    pub(crate) source: DataSource,
    pub(crate) source_round: u64,
    pub(crate) warnings: Vec<String>,
}
