//! External AMM liquidity adapters.
//!
//! This module never fabricates pool state. Each response is derived from live
//! algod state and network-scoped protocol manifests. Write paths use the same
//! prepare/submit intent boundary as native LP actions and validate every
//! protocol-specific transaction field before signing.

use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::time::Duration;

use crate::api_error::{
    bad_request, internal, not_found, service_unavailable, ApiErrorResponse, ApiResult,
};
use axum::extract::{Extension, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use opennodia_amm::{AddLiquidityQuote, RemoveLiquidityQuote, SwapQuote};
use opennodia_core::{Address, MicroAlgo, Network, Round};
use opennodia_node::{AccountInfo, AlgodClient, TealKeyValue, TealValue};
#[cfg(test)]
use opennodia_swap::TransactionType;
use opennodia_swap::{fetch_tx_params, TransactionParams};

use crate::intent::IntentStoreError;
use crate::routes::verify_pin;
use crate::session::Session;
use crate::state::AppState;
use crate::tx_flow::{self, WalletTxGroup};

mod accounts;
mod drafts;
mod dto;
mod folks;
mod math;
mod pact;
mod positions;
mod quote_math;
mod read_handlers;
mod sources;
mod teal_state;
mod tinyman;
mod tx_groups;
mod write_handlers;
use accounts::{
    account_asset_balance, available_algo, confirmed_asset_increase, fetch_account,
    require_can_receive, require_can_send,
};
pub use dto::*;
use math::{mul_div_ceil, mul_div_floor};
use pact::{discover_pact_constant_product_pools, read_pact_constant_product_pool};
use positions::external_position_response;
use quote_math::{
    external_add_minted_floor, quote_external_balanced_add, quote_external_exact_in,
    quote_external_remove,
};
#[cfg(test)]
use quote_math::{normalize_pair_reserves, quote_input_fee_cpmm, quote_output_fee_cpmm};
pub(crate) use read_handlers::external_route_quote_candidates;
use read_handlers::{
    external_liquidity_status, list_external_pools, list_external_positions, quote_external_pool,
};
use sources::{requested_sources, source_statuses, ExternalManifest, ExternalSource};
use teal_state::{
    decode_u64_list, optional_state_bytes_raw_key, optional_state_text, optional_state_uint,
    state_bytes, state_uint, teal_state_map, uint_from_be_bytes,
};
use tinyman::{read_tinyman_v2_pool_by_address, read_tinyman_v2_pool_by_pair};
use tx_groups::{
    build_external_add_group, build_external_remove_group, build_external_swap_group,
    ensure_external_pool_liquidity_writable, external_add_foreign_assets,
    external_remove_foreign_assets, same_external_pool, validate_external_add_group,
    validate_external_remove_group, validate_external_swap_group,
};
use write_handlers::{
    prepare_external_add, prepare_external_remove, submit_external_add, submit_external_remove,
};
pub(crate) use write_handlers::{prepare_external_swap, submit_external_swap};

const CONFIRMATION_TIMEOUT_ROUNDS: u64 = 20;
const DEFAULT_EXTERNAL_SWAP_EXPIRE_ROUNDS: u64 = 1_000;

/// Build the external-liquidity sub-router. Mounted under the protected layer.
pub fn external_liquidity_router() -> Router<AppState> {
    Router::new()
        .route("/api/lp/external/status", get(external_liquidity_status))
        .route("/api/lp/external/pools", get(list_external_pools))
        .route("/api/lp/external/positions", get(list_external_positions))
        .route("/api/lp/external/quote", post(quote_external_pool))
        .route("/api/lp/external/swap/prepare", post(prepare_external_swap))
        .route("/api/lp/external/swap", post(submit_external_swap))
        .route("/api/lp/external/add/prepare", post(prepare_external_add))
        .route("/api/lp/external/add", post(submit_external_add))
        .route(
            "/api/lp/external/remove/prepare",
            post(prepare_external_remove),
        )
        .route("/api/lp/external/remove", post(submit_external_remove))
}

#[derive(Debug, Clone)]
struct ExternalPoolState {
    response: ExternalPoolResponse,
    quote_math: ExternalQuoteMath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExternalQuoteMath {
    TinymanV2InputFee,
    PactConstantProductOutputFee,
}

fn pool_response_with_capabilities(
    mut pool: ExternalPoolResponse,
    swap_enabled: bool,
    _liquidity_enabled: bool,
) -> ExternalPoolResponse {
    pool.position_supported = pool.tradable;
    let pool_swap_supported =
        pool.tradable && swap_enabled && pool.adapter_swap_supported && pool.folks.is_none();
    pool.swap_supported = pool_swap_supported;
    if pool.tradable {
        pool.status = if pool_swap_supported {
            "swap_enabled".into()
        } else {
            "quote_only".into()
        };
    }
    pool
}

async fn read_external_pool_by_id(
    algod: &AlgodClient,
    network: Network,
    source: &str,
    pool_id: &str,
) -> ApiResult<ExternalPoolState> {
    let source = ExternalSource::parse(source)?;
    let manifest = ExternalManifest::for_network(network);
    let round = algod
        .status()
        .await
        .map_err(|error| service_unavailable(format!("fetch algod status: {error}")))?
        .last_round
        .as_u64();
    match source {
        ExternalSource::Tinyman => {
            let address = Address::from_str(pool_id.trim())
                .map_err(|error| bad_request(format!("invalid Tinyman pool address: {error}")))?;
            read_tinyman_v2_pool_by_address(algod, manifest, address, round)
                .await?
                .ok_or_else(|| bad_request("Tinyman pool was not found or is not initialized"))
        }
        ExternalSource::Pact => {
            let app_id = pool_id
                .trim()
                .parse::<u64>()
                .map_err(|error| bad_request(format!("invalid Pact pool app ID: {error}")))?;
            read_pact_constant_product_pool(algod, manifest, app_id, round).await
        }
    }
}

fn tx_params_with_deadline(
    mut params: TransactionParams,
    expire_rounds: u64,
) -> ApiResult<(TransactionParams, Round)> {
    let deadline = params.first_valid + expire_rounds;
    params.last_valid = deadline;
    Ok((params, deadline))
}

async fn store_external_intent(
    state: &AppState,
    session: &Session,
    wallet_id: &str,
    intent: ExternalLiquidityIntentAction,
) -> ApiResult<String> {
    if !state.stores.wallets.contains_wallet(wallet_id).await {
        return Err(not_found(format!("wallet not found: {wallet_id}")));
    }
    let ttl = Duration::from_secs(state.config.dex.intent_ttl_secs.max(30));
    state
        .intents
        .external_liquidity
        .store(&session.sid, wallet_id, ttl, intent)
        .await
        .map_err(intent_error)
}

async fn take_external_intent(
    state: &AppState,
    session: &Session,
    wallet_id: &str,
    intent_id: &str,
) -> ApiResult<ExternalLiquidityIntentAction> {
    state
        .intents
        .external_liquidity
        .take(&session.sid, wallet_id, intent_id)
        .await
        .map_err(intent_error)
}

fn intent_error(error: IntentStoreError) -> ApiErrorResponse {
    crate::api_error::intent_store_error(error, "external liquidity")
}

fn ensure_external_swaps_enabled(state: &AppState) -> ApiResult<()> {
    if state.config.external_liquidity.swap_enabled {
        return Ok(());
    }
    Err(service_unavailable(
        "external AMM swaps are disabled by external_liquidity.swap_enabled=false; discovery and quote remain read-only",
    ))
}

fn ensure_external_liquidity_enabled(state: &AppState) -> ApiResult<()> {
    if state.config.external_liquidity.liquidity_enabled {
        return Ok(());
    }
    Err(service_unavailable(
        "external AMM LP add/remove is disabled by external_liquidity.liquidity_enabled=false; discovery, quote, and position reads remain read-only",
    ))
}

fn api_error(error: &ApiErrorResponse) -> String {
    error.1 .0.error.clone()
}

#[cfg(test)]
mod tests {
    use super::folks::{current_deposit_interest_index, parse_folks_manager_pool_info};
    use super::pact::pact_pool_box_name;
    use super::sources::TINYMAN_V2_TESTNET_VALIDATOR_APP_ID;
    use super::tinyman::tinyman_v2_pool_address;
    use super::*;
    use opennodia_core::Round;

    fn tx_params() -> TransactionParams {
        TransactionParams::new(Round(1_000), "testnet-v1.0".into(), [7; 32])
    }

    fn external_pool(source: &str) -> ExternalPoolResponse {
        ExternalPoolResponse {
            pool_id: if source == "tinyman" {
                "POOLADDR".into()
            } else {
                "123".into()
            },
            source: source.into(),
            app_id: if source == "tinyman" {
                TINYMAN_V2_TESTNET_VALIDATOR_APP_ID
            } else {
                123
            },
            app_address: Address::from_bytes([3; 32]).to_string(),
            lp_asset_id: 77,
            asset_0: 0,
            asset_1: 42,
            fee_bps: 30,
            protocol_fee_bps: None,
            protocol_version: "v2".into(),
            reserve_0: 10_000_000,
            reserve_1: 20_000_000,
            total_lp_supply: 1_000_000,
            source_round: 999,
            quote_supported: true,
            swap_supported: true,
            adapter_swap_supported: true,
            position_supported: true,
            tradable: true,
            folks_backed: false,
            folks: None,
            status: "swap_enabled".into(),
            status_note: "test".into(),
        }
    }

    fn swap_quote(pool_id: &str) -> SwapQuote {
        SwapQuote {
            pool_id: pool_id.into(),
            asset_in: 0,
            asset_out: 42,
            amount_in: 1_000_000,
            amount_out: 1_900_000,
            minimum_out: 1_890_000,
            fee_bps: 30,
            fee_amount_estimate: 3_000,
            price_impact_bps: 50,
            source_round: 999,
        }
    }

    #[test]
    fn tinyman_v2_pool_address_is_stable() {
        let address = tinyman_v2_pool_address(TINYMAN_V2_TESTNET_VALIDATOR_APP_ID, 10, 8)
            .unwrap()
            .to_string();
        assert_eq!(address.len(), 58);
        assert_eq!(
            address,
            tinyman_v2_pool_address(TINYMAN_V2_TESTNET_VALIDATOR_APP_ID, 8, 10)
                .unwrap()
                .to_string()
        );
    }

    #[test]
    fn pact_pool_box_name_uses_four_uint64_values() {
        let box_name = pact_pool_box_name(0, 31566704, 30, 201);
        assert_eq!(box_name.len(), 32);
        assert_eq!(&box_name[0..8], &0u64.to_be_bytes());
        assert_eq!(&box_name[8..16], &31566704u64.to_be_bytes());
        assert_eq!(&box_name[16..24], &30u64.to_be_bytes());
        assert_eq!(&box_name[24..32], &201u64.to_be_bytes());
    }

    #[test]
    fn tinyman_input_fee_quote_matches_sdk_vector() {
        let (amount_out, fee) =
            quote_input_fee_cpmm(10_000_000, 1_000_000_000, 10_000_000, 30).unwrap();
        assert_eq!(fee, 30_000);
        assert_eq!(amount_out, 499_248_873);
    }

    #[test]
    fn pact_output_fee_quote_matches_constant_product_formula() {
        let (amount_out, fee) =
            quote_output_fee_cpmm(10_000_000, 1_000_000_000, 10_000_000, 30).unwrap();
        assert_eq!(amount_out, 498_500_000);
        assert_eq!(fee, 1_500_000);
    }

    #[test]
    fn decode_uint64_list_rejects_misaligned_bytes() {
        assert!(decode_u64_list(&[1, 2, 3]).is_err());
        assert_eq!(decode_u64_list(&[0, 0, 0, 0, 0, 0, 0, 7]).unwrap(), vec![7]);
    }

    #[test]
    fn parses_folks_manager_pool_info_offsets() {
        let mut chunk = [0u8; 42];
        chunk[0..6].copy_from_slice(&[0, 0, 0, 0, 0x12, 0x34]);
        chunk[22..30].copy_from_slice(&5u64.to_be_bytes());
        chunk[30..38].copy_from_slice(&7u64.to_be_bytes());
        chunk[38..42].copy_from_slice(&9u32.to_be_bytes());

        let info = parse_folks_manager_pool_info(&chunk).unwrap();
        assert_eq!(info.pool_app_id, 0x1234);
        assert_eq!(info.deposit_interest_rate, 5);
        assert_eq!(info.deposit_interest_index, 7);
        assert_eq!(info.updated_at, 9);
    }

    #[test]
    fn folks_deposit_interest_index_uses_linear_accrual() {
        let base = 100_000_000_000_000u64;
        let rate = 1_000_000_000_000_000u64;
        let one_year = 31_536_000u64;

        let current = current_deposit_interest_index(rate, base, 100, 100 + one_year).unwrap();
        assert_eq!(current, 110_000_000_000_000);
    }

    #[test]
    fn normalizes_reserves_by_asset_id() {
        assert_eq!(normalize_pair_reserves(10, 8, 100, 200), (8, 10, 200, 100));
    }

    #[test]
    fn tinyman_swap_group_is_locally_validated() {
        let pool = external_pool("tinyman");
        let quote = swap_quote(&pool.pool_id);
        let trader = Address::from_bytes([9; 32]);
        let txs = build_external_swap_group(&pool, &quote, trader, &tx_params()).unwrap();
        let group = WalletTxGroup::new(trader, txs).unwrap();

        validate_external_swap_group(&pool, &quote, trader, &group, "test").unwrap();
        assert_eq!(group.txs()[1].app_args[0], b"swap".to_vec());
        assert_eq!(group.txs()[1].foreign_assets, vec![42, 0]);
    }

    #[test]
    fn pact_swap_group_is_locally_validated() {
        let pool = external_pool("pact");
        let quote = swap_quote(&pool.pool_id);
        let trader = Address::from_bytes([9; 32]);
        let txs = build_external_swap_group(&pool, &quote, trader, &tx_params()).unwrap();
        let group = WalletTxGroup::new(trader, txs).unwrap();

        validate_external_swap_group(&pool, &quote, trader, &group, "test").unwrap();
        assert_eq!(group.txs()[1].app_args[0], b"SWAP".to_vec());
        assert_eq!(group.txs()[1].foreign_assets, vec![0, 42]);
    }

    #[test]
    fn external_swap_validation_rejects_minimum_out_mutation() {
        let pool = external_pool("pact");
        let quote = swap_quote(&pool.pool_id);
        let trader = Address::from_bytes([9; 32]);
        let mut txs = build_external_swap_group(&pool, &quote, trader, &tx_params()).unwrap();
        txs[1].app_args[1] = 1u64.to_be_bytes().to_vec();
        let group = WalletTxGroup::new(trader, txs).unwrap();

        assert!(validate_external_swap_group(&pool, &quote, trader, &group, "test").is_err());
    }

    #[test]
    fn tinyman_add_group_is_locally_validated() {
        let pool = external_pool("tinyman");
        let quote = quote_external_balanced_add(&pool, 1_000_000, 2_000_000, 50).unwrap();
        let provider = Address::from_bytes([9; 32]);
        let txs = build_external_add_group(&pool, &quote, provider, &tx_params()).unwrap();
        let group = WalletTxGroup::new(provider, txs).unwrap();

        validate_external_add_group(&pool, &quote, provider, &group, "test").unwrap();
        assert_eq!(group.txs()[0].xfer_asset, Some(42));
        assert_eq!(group.txs()[1].ty, TransactionType::Pay);
        assert_eq!(group.txs()[2].app_args[0], b"add_liquidity".to_vec());
        assert_eq!(group.txs()[2].app_args[1], b"flexible".to_vec());
        assert_eq!(group.txs()[2].foreign_assets, vec![77]);
    }

    #[test]
    fn pact_add_group_is_locally_validated() {
        let pool = external_pool("pact");
        let quote = quote_external_balanced_add(&pool, 1_000_000, 2_000_000, 50).unwrap();
        let provider = Address::from_bytes([9; 32]);
        let txs = build_external_add_group(&pool, &quote, provider, &tx_params()).unwrap();
        let group = WalletTxGroup::new(provider, txs).unwrap();

        validate_external_add_group(&pool, &quote, provider, &group, "test").unwrap();
        assert_eq!(group.txs()[0].ty, TransactionType::Pay);
        assert_eq!(group.txs()[1].xfer_asset, Some(42));
        assert_eq!(group.txs()[2].app_args[0], b"ADDLIQ".to_vec());
        assert_eq!(group.txs()[2].foreign_assets, vec![0, 42, 77]);
    }

    #[test]
    fn tinyman_remove_group_is_locally_validated() {
        let pool = external_pool("tinyman");
        let quote = quote_external_remove(&pool, 100_000, 50).unwrap();
        let provider = Address::from_bytes([9; 32]);
        let txs = build_external_remove_group(&pool, &quote, provider, &tx_params()).unwrap();
        let group = WalletTxGroup::new(provider, txs).unwrap();

        validate_external_remove_group(&pool, &quote, provider, &group, "test").unwrap();
        assert_eq!(group.txs()[0].xfer_asset, Some(77));
        assert_eq!(group.txs()[1].app_args[0], b"remove_liquidity".to_vec());
        assert_eq!(group.txs()[1].foreign_assets, vec![42, 0]);
    }

    #[test]
    fn pact_remove_group_is_locally_validated() {
        let pool = external_pool("pact");
        let quote = quote_external_remove(&pool, 100_000, 50).unwrap();
        let provider = Address::from_bytes([9; 32]);
        let txs = build_external_remove_group(&pool, &quote, provider, &tx_params()).unwrap();
        let group = WalletTxGroup::new(provider, txs).unwrap();

        validate_external_remove_group(&pool, &quote, provider, &group, "test").unwrap();
        assert_eq!(group.txs()[0].xfer_asset, Some(77));
        assert_eq!(group.txs()[1].app_args[0], b"REMLIQ".to_vec());
        assert_eq!(group.txs()[1].foreign_assets, vec![0, 42]);
    }

    #[test]
    fn external_add_validation_rejects_minimum_lp_mutation() {
        let pool = external_pool("pact");
        let quote = quote_external_balanced_add(&pool, 1_000_000, 2_000_000, 50).unwrap();
        let provider = Address::from_bytes([9; 32]);
        let mut txs = build_external_add_group(&pool, &quote, provider, &tx_params()).unwrap();
        txs[2].app_args[1] = 1u64.to_be_bytes().to_vec();
        let group = WalletTxGroup::new(provider, txs).unwrap();

        assert!(validate_external_add_group(&pool, &quote, provider, &group, "test").is_err());
    }

    #[test]
    fn external_remove_validation_rejects_minimum_mutation() {
        let pool = external_pool("tinyman");
        let quote = quote_external_remove(&pool, 100_000, 50).unwrap();
        let provider = Address::from_bytes([9; 32]);
        let mut txs = build_external_remove_group(&pool, &quote, provider, &tx_params()).unwrap();
        txs[1].app_args[1] = 1u64.to_be_bytes().to_vec();
        let group = WalletTxGroup::new(provider, txs).unwrap();

        assert!(validate_external_remove_group(&pool, &quote, provider, &group, "test").is_err());
    }

    #[test]
    fn pool_capabilities_keep_unverified_adapter_read_only() {
        let mut pool = external_pool("pact");
        pool.adapter_swap_supported = false;
        let response = pool_response_with_capabilities(pool, true, true);
        assert!(response.tradable);
        assert!(!response.swap_supported);
        assert_eq!(response.status, "quote_only");
    }

    #[test]
    fn pool_capabilities_keep_folks_backed_pool_read_only() {
        let mut pool = external_pool("pact");
        pool.folks_backed = true;
        pool.folks = Some(FolksBackedInfo {
            source: "test".into(),
            adapter_app_id: Some(1),
            pool_0_app_id: 2,
            pool_1_app_id: 3,
            underlying_0: 0,
            underlying_1: 42,
            f_asset_0: 10,
            f_asset_1: 11,
            deposit_interest_rate_0: 0,
            deposit_interest_rate_1: 0,
            deposit_interest_index_0: 0,
            deposit_interest_index_1: 0,
            redeem_available_0: 0,
            redeem_available_1: 0,
            f_asset_outstanding_0: 0,
            f_asset_outstanding_1: 0,
            total_deposit_0: 0,
            total_deposit_1: 0,
            total_borrowed_0: 0,
            total_borrowed_1: 0,
            utilization_bps_0: 0,
            utilization_bps_1: 0,
            utilization_available: false,
            utilization_note: "test".into(),
            risk_note: "test".into(),
        });
        let response = pool_response_with_capabilities(pool, true, true);
        assert!(response.tradable);
        assert!(!response.swap_supported);
        assert_eq!(response.status, "quote_only");
    }
}
