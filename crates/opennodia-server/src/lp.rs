//! LP Trade API endpoints.
//!
//! These endpoints only derive pool data from live algod/indexer state. If an
//! application does not expose the OpenNodia native AMM global-state schema, it
//! is rejected instead of being displayed as a pool.

use std::collections::{HashMap, HashSet};

use axum::extract::{Extension, Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use opennodia_amm::transactions::{
    build_pool_add_liquidity, build_pool_bootstrap, build_pool_create, build_pool_remove_liquidity,
    build_pool_setup, build_pool_swap, build_registered_pool_create, AddLiquidityRequest,
    BootstrapRequest, PoolCreateRequest, PoolGroupDraft, PoolSetupRequest,
    RegisteredPoolCreateRequest, RemoveLiquidityRequest, SwapRequest,
};
use opennodia_amm::{
    quote_balanced_add, quote_exact_in, quote_initial_liquidity, quote_remove, AddLiquidityQuote,
    FeeTier, PoolKey, PoolState, RemoveLiquidityQuote, SwapQuote,
};
use opennodia_core::{Address, MicroAlgo, Round};
use opennodia_node::{AccountInfo, AlgodClient, ApplicationInfo};
use opennodia_swap::{fetch_tx_params, TransactionFields, TransactionParams};
use serde::Deserialize;

use crate::api_error::{
    bad_request, internal, not_found, service_unavailable, ApiErrorResponse, ApiResult,
};
use crate::intent::IntentStoreError;
use crate::routes::verify_pin;
use crate::session::Session;
use crate::state::AppState;
use crate::tx_flow::{self, WalletTxGroup};

const CONFIRMATION_TIMEOUT_ROUNDS: u64 = 20;

mod accounts;
mod contracts;
mod drafts;
mod dto;
mod guards;
mod intent;
mod pools;
mod prepare_handlers;
mod quote_handler;
mod registry;
mod route_quotes;
mod submit_handlers;
use accounts::{available_algo, fetch_account, require_can_receive, require_can_send};
use contracts::{
    compile_native_pool_programs, compile_native_registry_programs, current_pool_approval_program,
    genesis_hash, validate_native_registry_app,
};
pub(crate) use drafts::pool_swap_draft;
pub use dto::*;
#[cfg(test)]
use guards::native_amm_writes_allowed_for;
use guards::{
    authority_is_enabled, ensure_native_amm_writes_allowed, native_amm_write_status_note,
    native_amm_writes_allowed,
};
pub(crate) use intent::LpIntentAction;
#[cfg(test)]
use opennodia_core::Network;
use pools::{
    decode_application_pool, ensure_pool_is_tradable, pool_is_tradable, pool_response,
    position_response, read_pool, record_pool_state, reject_network_duplicate_pool,
    reject_registered_duplicate_pool, reject_registry_duplicate_pool, require_prepared_pool_state,
};
pub(crate) use pools::{pool_execution_state_matches, read_current_pool};
pub(crate) use prepare_handlers::prepare_pool_swap;
use prepare_handlers::{
    prepare_pool_add, prepare_pool_bootstrap, prepare_pool_create, prepare_pool_remove,
    prepare_pool_setup, prepare_registry_create,
};
use quote_handler::pool_quote;
pub use registry::{LpRegistry, LpRegistryEntry};
pub(crate) use route_quotes::native_route_quote_candidates;
pub(crate) use submit_handlers::swap_pool_exact_in;
use submit_handlers::{
    add_pool_liquidity, bootstrap_pool, create_pool, create_registry, remove_pool_liquidity,
    setup_pool,
};

/// Build the LP Trade sub-router. Mounted under the protected auth layer.
pub fn lp_router() -> Router<AppState> {
    Router::new()
        .route("/api/lp/status", get(lp_status))
        .route(
            "/api/lp/registry/create/prepare",
            post(prepare_registry_create),
        )
        .route("/api/lp/registry/create", post(create_registry))
        .route("/api/lp/positions", get(list_positions))
        .route("/api/lp/pools", get(list_pools))
        .route("/api/lp/pools/{app_id}", get(pool_detail))
        .route("/api/lp/quote", post(pool_quote))
        .route("/api/lp/pools/create/prepare", post(prepare_pool_create))
        .route("/api/lp/pools/create", post(create_pool))
        .route("/api/lp/pools/setup/prepare", post(prepare_pool_setup))
        .route("/api/lp/pools/setup", post(setup_pool))
        .route(
            "/api/lp/pools/bootstrap/prepare",
            post(prepare_pool_bootstrap),
        )
        .route("/api/lp/pools/bootstrap", post(bootstrap_pool))
        .route("/api/lp/pools/add/prepare", post(prepare_pool_add))
        .route("/api/lp/pools/add", post(add_pool_liquidity))
        .route("/api/lp/pools/remove/prepare", post(prepare_pool_remove))
        .route("/api/lp/pools/remove", post(remove_pool_liquidity))
        .route("/api/lp/swap/prepare", post(prepare_pool_swap))
        .route("/api/lp/swap", post(swap_pool_exact_in))
}

fn api_error(error: &ApiErrorResponse) -> String {
    error.1.error.clone()
}

fn parse_address(value: &str, field: &str) -> ApiResult<Address> {
    value
        .parse::<Address>()
        .map_err(|error| bad_request(format!("invalid {field} address: {error}")))
}

async fn reject_regulated_asset(algod: &AlgodClient, asset_id: u64) -> ApiResult<()> {
    if asset_id == 0 {
        return Ok(());
    }
    let params = algod
        .asset_params(asset_id)
        .await
        .map_err(|error| bad_request(format!("asset {asset_id} not found: {error}")))?;
    if params.total == 0 || params.destroyed_at_round != 0 {
        return Err(bad_request(format!("asset {asset_id} is not active")));
    }
    let grade = opennodia_assets::AssetPolicyGrade::classify(
        authority_is_enabled(&params.freeze)?,
        authority_is_enabled(&params.clawback)?,
        params.default_frozen,
    );
    if !grade.is_tradeable_by_default() {
        return Err(bad_request(format!(
            "asset {asset_id} is regulated (freeze/clawback/default-frozen) and cannot be used for native pools"
        )));
    }
    Ok(())
}

async fn validate_pool_assets(algod: &AlgodClient, asset_a: u64, asset_b: u64) -> ApiResult<()> {
    if asset_a == asset_b {
        return Err(bad_request("pool assets must differ"));
    }
    reject_regulated_asset(algod, asset_a).await?;
    reject_regulated_asset(algod, asset_b).await?;
    Ok(())
}

async fn fetch_params(algod: &AlgodClient) -> ApiResult<opennodia_swap::TransactionParams> {
    fetch_tx_params(algod)
        .await
        .map_err(|error| service_unavailable(format!("fetch transaction params: {error}")))
}

fn total_fee(txs: &[TransactionFields]) -> u64 {
    txs.iter().map(|tx| tx.fee).sum()
}

fn wallet_group(signer: Address, txs: Vec<TransactionFields>) -> ApiResult<WalletTxGroup> {
    WalletTxGroup::new(signer, txs)
}

async fn store_lp_intent(
    state: &AppState,
    session: &Session,
    wallet_id: &str,
    intent: LpIntentAction,
) -> ApiResult<String> {
    if !state.stores.wallets.contains_wallet(wallet_id).await {
        return Err(not_found(format!("wallet not found: {wallet_id}")));
    }

    let ttl = std::time::Duration::from_secs(state.config.dex.intent_ttl_secs.max(30));
    state
        .intents
        .lp
        .store(&session.sid, wallet_id, ttl, intent)
        .await
        .map_err(lp_intent_error)
}

async fn take_lp_intent(
    state: &AppState,
    session: &Session,
    wallet_id: &str,
    intent_id: &str,
) -> ApiResult<LpIntentAction> {
    state
        .intents
        .lp
        .take(&session.sid, wallet_id, intent_id)
        .await
        .map_err(lp_intent_error)
}

fn lp_intent_error(error: IntentStoreError) -> ApiErrorResponse {
    crate::api_error::intent_store_error(error, "LP")
}

fn draft_foreign_assets(draft: &PoolGroupDraft) -> Vec<u64> {
    draft
        .txs
        .last()
        .map(|tx| tx.foreign_assets.clone())
        .unwrap_or_default()
}

fn tx_params_with_deadline(
    mut params: TransactionParams,
    expire_rounds: u64,
) -> ApiResult<(TransactionParams, Round)> {
    if expire_rounds == 0 || expire_rounds > 1_000 {
        return Err(bad_request(
            "expire_rounds must be between 1 and 1000 rounds",
        ));
    }
    let deadline = params.first_valid + expire_rounds;
    params.last_valid = deadline;
    Ok((params, deadline))
}

fn bootstrap_safety_preview(
    pool: &PoolState,
    quote: &AddLiquidityQuote,
    draft: &PoolGroupDraft,
    account: &AccountInfo,
) -> PoolBootstrapSafetyPreview {
    let network_fee = total_fee(&draft.txs);
    PoolBootstrapSafetyPreview {
        initial_price_numerator: quote.amount_1,
        initial_price_denominator: quote.amount_0,
        initial_price_display: format!(
            "{} raw asset {} per {} raw asset {}",
            quote.amount_1, pool.key.asset_1, quote.amount_0, pool.key.asset_0
        ),
        price_impact_bps: 0,
        provider_current_microalgo: account.amount,
        provider_min_balance_microalgo: account.min_balance,
        provider_available_microalgo: available_algo(account),
        network_fee_microalgo: network_fee,
        will_be_tradable: quote.amount_0 > 0 && quote.amount_1 > 0 && quote.minted_lp > 0,
        note: "bootstrap is a single atomic group: asset 0 deposit, asset 1 deposit, then LP mint app call"
            .into(),
    }
}

async fn fetch_created_app_id(algod: &AlgodClient, txid: &str) -> ApiResult<u64> {
    #[derive(Debug, Deserialize)]
    struct PendingResp {
        #[serde(rename = "application-index", default)]
        application_index: u64,
    }

    let url = format!("{}/v2/transactions/pending/{txid}", algod.base_url());
    let resp = reqwest::Client::new()
        .get(&url)
        .header("X-Algo-API-Token", algod.token())
        .send()
        .await
        .map_err(|error| internal(format!("fetch pending transaction: {error}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(internal(format!(
            "fetch pending transaction {status}: {body}"
        )));
    }
    let pending: PendingResp = resp
        .json()
        .await
        .map_err(|error| internal(format!("decode pending transaction: {error}")))?;
    if pending.application_index == 0 {
        return Err(internal(format!(
            "confirmed app creation {txid} did not include an application-index"
        )));
    }
    Ok(pending.application_index)
}

async fn lp_status(State(state): State<AppState>) -> Json<LpStatusResponse> {
    let writes_allowed = native_amm_writes_allowed(&state);
    Json(LpStatusResponse {
        native_contract_version: opennodia_amm::CURRENT_CONTRACT_VERSION,
        curve_id: opennodia_amm::CURVE_CPMM_V1,
        fee_tiers_bps: vec![
            FeeTier::STABLE_005.bps(),
            FeeTier::STANDARD_030.bps(),
            FeeTier::VOLATILE_100.bps(),
        ],
        create_setup_enabled: writes_allowed,
        swap_liquidity_enabled: writes_allowed,
        mainnet_write_enabled_after_audit: state.config.lp.mainnet_write_enabled_after_audit,
        write_status_note: native_amm_write_status_note(&state),
        native_registry_app_id: state.config.lp.native_registry_app_id,
        registry_required: state.config.lp.require_registry,
    })
}

async fn list_positions(
    State(state): State<AppState>,
    Query(query): Query<PositionQuery>,
) -> ApiResult<Json<LpPositionsResponse>> {
    let address = parse_address(&query.address, "address")?;
    let (algod, status, source) = state
        .authoritative_ledger()
        .await
        .map_err(service_unavailable)?;
    let round = status.last_round.as_u64();
    let genesis_hash = genesis_hash(algod).await?;
    let expected_programs = compile_native_pool_programs(algod).await?;
    let account = fetch_account(algod, address).await?;
    let holdings: HashMap<u64, u64> = account
        .assets
        .iter()
        .filter(|holding| holding.amount > 0 && !holding.is_frozen)
        .map(|holding| (holding.asset_id, holding.amount))
        .collect();
    if holdings.is_empty() {
        return Ok(Json(LpPositionsResponse {
            address: address.to_string(),
            positions: Vec::new(),
            source,
            source_round: round,
            discovery_note: Some("address has no non-zero ASA holdings".into()),
        }));
    }

    let registry_entries = {
        let registry = state.stores.lp_registry.lock().await;
        registry.entries()
    };
    let mut seen_app_ids = HashSet::new();
    let mut positions = Vec::new();
    let mut skipped = 0usize;
    for entry in registry_entries {
        if entry.lp_asset_id == 0 || !seen_app_ids.insert(entry.app_id) {
            continue;
        }
        let Some(lp_balance) = holdings.get(&entry.lp_asset_id).copied() else {
            continue;
        };
        match read_pool(algod, entry.app_id, genesis_hash, round, &expected_programs).await {
            Ok(pool) if pool.lp_asset_id == entry.lp_asset_id && pool_is_tradable(&pool) => {
                positions.push(position_response(&pool, lp_balance));
            }
            Ok(_) => skipped += 1,
            Err(error) => {
                skipped += 1;
                tracing::debug!(
                    app_id = entry.app_id,
                    error = ?error,
                    "skipping LP position candidate that could not be loaded"
                );
            }
        }
    }

    let discovery_note = if positions.is_empty() {
        Some(
            "no native LP positions found for this address in the local verified pool registry"
                .into(),
        )
    } else if skipped > 0 {
        Some(format!(
            "loaded {} position(s); skipped {skipped} stale registry entr{}",
            positions.len(),
            if skipped == 1 { "y" } else { "ies" }
        ))
    } else {
        None
    };

    Ok(Json(LpPositionsResponse {
        address: address.to_string(),
        positions,
        source,
        source_round: round,
        discovery_note,
    }))
}

async fn list_pools(
    State(state): State<AppState>,
    Query(query): Query<PoolListQuery>,
) -> ApiResult<Json<PoolListResponse>> {
    let (algod, status, source) = state
        .authoritative_ledger()
        .await
        .map_err(service_unavailable)?;
    let round = status.last_round.as_u64();
    let genesis_hash = genesis_hash(algod).await?;
    let expected_programs = compile_native_pool_programs(algod).await?;

    let Some(asset_a) = query.asset_a else {
        return Ok(Json(PoolListResponse {
            pools: Vec::new(),
            source,
            source_round: round,
            discovery_note: Some("asset_a is required for native pool discovery".into()),
        }));
    };
    let Some(asset_b) = query.asset_b else {
        return Ok(Json(PoolListResponse {
            pools: Vec::new(),
            source,
            source_round: round,
            discovery_note: Some("asset_b is required for native pool discovery".into()),
        }));
    };

    if asset_a == asset_b {
        return Err(bad_request("asset_a and asset_b must differ"));
    }

    let registry_entries = {
        let registry = state.stores.lp_registry.lock().await;
        registry.entries_for_pair(genesis_hash, asset_a, asset_b)
    };
    let mut seen_app_ids = HashSet::new();
    let mut pools = Vec::new();
    for entry in registry_entries {
        match read_pool(algod, entry.app_id, genesis_hash, round, &expected_programs).await {
            Ok(pool) => {
                if pool.key.contains(asset_a) && pool.key.contains(asset_b) {
                    seen_app_ids.insert(pool.app_id);
                    pools.push(pool_response(&pool));
                }
            }
            Err(_error) => {
                tracing::debug!(
                    app_id = entry.app_id,
                    "skipping registered LP pool that could not be loaded"
                );
            }
        }
    }

    let mut discovery_note = None;
    if let Some(indexer) = state
        .ledger
        .indexer
        .as_ref()
        .or(state.ledger.public_indexer.as_ref())
    {
        let discover_asset = match (asset_a, asset_b) {
            (0, 0) => unreachable!("equal assets rejected"),
            (0, other) | (other, 0) => other,
            (left, right) => left.min(right),
        };

        let applications = indexer
            .applications_by_asset(discover_asset)
            .await
            .map_err(|error| service_unavailable(format!("pool discovery failed: {error}")))?;
        for app in applications {
            if seen_app_ids.contains(&app.id) {
                continue;
            }
            let mut app_info = ApplicationInfo {
                id: app.id,
                params: opennodia_node::ApplicationParams {
                    creator: app.params.creator,
                    approval_program: app.params.approval_program,
                    clear_state_program: app.params.clear_state_program,
                    global_state: app.params.global_state,
                    global_state_schema: None,
                    local_state_schema: None,
                    extra_program_pages: 0,
                },
            };
            if app_info.params.global_state.is_empty()
                || app_info.params.approval_program.is_empty()
                || app_info.params.clear_state_program.is_empty()
            {
                app_info = match algod.application_info(app_info.id).await {
                    Ok(info) => info,
                    Err(error) => {
                        tracing::debug!(
                            app_id = app_info.id,
                            %error,
                            "skipping application candidate that could not be loaded from algod"
                        );
                        continue;
                    }
                };
            }
            let Ok(pool) =
                decode_application_pool(&app_info, genesis_hash, round, &expected_programs)
            else {
                continue;
            };
            if pool.key.contains(asset_a) && pool.key.contains(asset_b) {
                seen_app_ids.insert(pool.app_id);
                record_pool_state(&state, &pool).await;
                pools.push(pool_response(&pool));
            }
        }
    } else {
        discovery_note =
            Some("indexer is unavailable; showing only locally registered native pools".into());
    }

    if pools.is_empty() && discovery_note.is_none() {
        discovery_note = Some(
            "no registered native pools found; load a known app ID once to add it to the local registry"
                .into(),
        );
    }

    Ok(Json(PoolListResponse {
        pools,
        source,
        source_round: round,
        discovery_note,
    }))
}

async fn pool_detail(
    State(state): State<AppState>,
    Path(app_id): Path<u64>,
) -> ApiResult<Json<PoolDetailResponse>> {
    let (algod, status, source) = state
        .authoritative_ledger()
        .await
        .map_err(service_unavailable)?;
    let genesis_hash = genesis_hash(algod).await?;
    let expected_programs = compile_native_pool_programs(algod).await?;
    let pool = read_pool(
        algod,
        app_id,
        genesis_hash,
        status.last_round.as_u64(),
        &expected_programs,
    )
    .await?;
    record_pool_state(&state, &pool).await;
    let response = pool_response(&pool);

    Ok(Json(PoolDetailResponse {
        app_address: response.app_address.clone(),
        pool: response,
        source,
    }))
}

#[cfg(test)]
mod tests {
    use super::contracts::{
        current_pool_approval_program, pool_global_state, program_hash,
        verify_native_pool_programs, verify_native_registry_programs, verify_native_registry_state,
        NativePoolPrograms, NativeRegistryPrograms,
    };
    use super::*;
    use base64::Engine;
    use opennodia_amm::{
        PoolGlobalValue, GLOBAL_KEY_POOL_APPROVAL_HASH, GLOBAL_KEY_POOL_CLEAR_HASH,
        GLOBAL_KEY_REGISTRY_ACTIVE_COUNT, GLOBAL_KEY_REGISTRY_GENESIS_HASH,
        GLOBAL_KEY_REGISTRY_VERSION,
    };
    use opennodia_node::{ApplicationInfo, TealKeyValue, TealValue};

    use super::pools::decode_registry_box_app_id;

    fn b64(value: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(value)
    }

    #[test]
    fn native_amm_writes_stay_fail_closed_on_mainnet_without_audit_opt_in() {
        assert!(!native_amm_writes_allowed_for(Network::Mainnet, false));
        assert!(native_amm_writes_allowed_for(Network::Mainnet, true));
        assert!(native_amm_writes_allowed_for(Network::Testnet, false));
        assert!(native_amm_writes_allowed_for(Network::Betanet, false));
        assert!(native_amm_writes_allowed_for(Network::Local, false));
    }

    #[test]
    fn decodes_pool_global_state_values() {
        let entries = vec![
            TealKeyValue {
                key: b64(opennodia_amm::GLOBAL_KEY_ASSET_0),
                value: TealValue {
                    value_type: 2,
                    bytes: String::new(),
                    uint: 0,
                },
            },
            TealKeyValue {
                key: b64(opennodia_amm::GLOBAL_KEY_POOL_KEY),
                value: TealValue {
                    value_type: 1,
                    bytes: b64(&[9; 32]),
                    uint: 0,
                },
            },
        ];
        let state = pool_global_state(&entries).unwrap();
        assert_eq!(
            state.get(opennodia_amm::GLOBAL_KEY_ASSET_0),
            Some(&PoolGlobalValue::Uint(0))
        );
        assert_eq!(
            state.get(opennodia_amm::GLOBAL_KEY_POOL_KEY),
            Some(&PoolGlobalValue::Bytes(vec![9; 32]))
        );
    }

    #[test]
    fn rejects_unknown_teal_state_type() {
        let entries = vec![TealKeyValue {
            key: b64(b"bad"),
            value: TealValue {
                value_type: 99,
                bytes: String::new(),
                uint: 0,
            },
        }];
        assert!(pool_global_state(&entries).is_err());
    }

    #[test]
    fn rejects_native_pool_program_mismatch() {
        let app = ApplicationInfo {
            id: 7,
            params: opennodia_node::ApplicationParams {
                approval_program: b64(&[1, 2, 3]),
                clear_state_program: b64(&[4, 5, 6]),
                ..Default::default()
            },
        };
        let expected = NativePoolPrograms {
            approval_programs: vec![vec![9, 9, 9]],
            clear_state_program: vec![4, 5, 6],
        };

        assert!(verify_native_pool_programs(&app, &expected).is_err());
    }

    #[test]
    fn accepts_matching_native_pool_programs() {
        let app = ApplicationInfo {
            id: 7,
            params: opennodia_node::ApplicationParams {
                approval_program: b64(&[1, 2, 3]),
                clear_state_program: b64(&[4, 5, 6]),
                ..Default::default()
            },
        };
        let expected = NativePoolPrograms {
            approval_programs: vec![vec![1, 2, 3]],
            clear_state_program: vec![4, 5, 6],
        };

        verify_native_pool_programs(&app, &expected).unwrap();
    }

    #[test]
    fn decodes_registry_box_app_id() {
        let encoded = b64(&1234u64.to_be_bytes());
        assert_eq!(decode_registry_box_app_id(&encoded, 9).unwrap(), 1234);

        assert!(decode_registry_box_app_id(&b64(&[1, 2, 3]), 9).is_err());
        assert!(decode_registry_box_app_id(&b64(&0u64.to_be_bytes()), 9).is_err());
    }

    #[test]
    fn lp_position_uses_actual_lp_holding_share() {
        let key = PoolKey::new(
            [1; 32],
            0,
            42,
            FeeTier::STANDARD_030,
            opennodia_amm::CURRENT_CONTRACT_VERSION,
        )
        .unwrap();
        let pool = PoolState {
            key,
            source: opennodia_amm::PoolSource::Native,
            app_id: 7,
            lp_asset_id: 99,
            reserve_0: 1_000_000,
            reserve_1: 2_000_000,
            total_lp_supply: 10_000,
            source_round: 123,
        };
        let position = position_response(&pool, 2_500);

        assert_eq!(position.lp_balance, 2_500);
        assert_eq!(position.pool_share_ppm, 250_000);
        assert_eq!(position.underlying_0, 250_000);
        assert_eq!(position.underlying_1, 500_000);
        assert!(!position.cost_basis_available);
        assert!(!position.pnl_available);
    }

    #[test]
    fn pool_response_marks_only_two_sided_bootstrapped_pool_as_tradable() {
        let key = PoolKey::new(
            [1; 32],
            0,
            42,
            FeeTier::STANDARD_030,
            opennodia_amm::CURRENT_CONTRACT_VERSION,
        )
        .unwrap();
        let mut pool = PoolState {
            key,
            source: opennodia_amm::PoolSource::Native,
            app_id: 7,
            lp_asset_id: 0,
            reserve_0: 0,
            reserve_1: 0,
            total_lp_supply: 0,
            source_round: 123,
        };

        let created = pool_response(&pool);
        assert_eq!(created.lifecycle, "created");
        assert!(!created.tradable);
        assert!(created.setup_required);

        pool.lp_asset_id = 99;
        let ready = pool_response(&pool);
        assert_eq!(ready.lifecycle, "ready_for_bootstrap");
        assert!(!ready.tradable);
        assert!(ready.bootstrap_required);

        pool.reserve_0 = 1;
        let partial = pool_response(&pool);
        assert_eq!(partial.lifecycle, "invalid_partial_liquidity");
        assert!(!partial.tradable);

        pool.reserve_1 = 2;
        pool.total_lp_supply = 3;
        let bootstrapped = pool_response(&pool);
        assert_eq!(bootstrapped.lifecycle, "bootstrapped");
        assert!(bootstrapped.tradable);
        assert!(bootstrapped.active_liquidity);
    }

    #[test]
    fn accepts_matching_native_registry_programs_and_hashes() {
        let pool_programs = NativePoolPrograms {
            approval_programs: vec![vec![7, 8, 9]],
            clear_state_program: vec![10, 11, 12],
        };
        let app = ApplicationInfo {
            id: 7,
            params: opennodia_node::ApplicationParams {
                approval_program: b64(&[1, 2, 3]),
                clear_state_program: b64(&[4, 5, 6]),
                global_state: vec![
                    TealKeyValue {
                        key: b64(GLOBAL_KEY_REGISTRY_VERSION),
                        value: TealValue {
                            value_type: 2,
                            bytes: String::new(),
                            uint: 1,
                        },
                    },
                    TealKeyValue {
                        key: b64(GLOBAL_KEY_POOL_APPROVAL_HASH),
                        value: TealValue {
                            value_type: 1,
                            bytes: b64(&program_hash(
                                current_pool_approval_program(&pool_programs).unwrap(),
                            )),
                            uint: 0,
                        },
                    },
                    TealKeyValue {
                        key: b64(GLOBAL_KEY_POOL_CLEAR_HASH),
                        value: TealValue {
                            value_type: 1,
                            bytes: b64(&program_hash(&pool_programs.clear_state_program)),
                            uint: 0,
                        },
                    },
                    TealKeyValue {
                        key: b64(GLOBAL_KEY_REGISTRY_GENESIS_HASH),
                        value: TealValue {
                            value_type: 1,
                            bytes: b64(&[42; 32]),
                            uint: 0,
                        },
                    },
                    TealKeyValue {
                        key: b64(GLOBAL_KEY_REGISTRY_ACTIVE_COUNT),
                        value: TealValue {
                            value_type: 2,
                            bytes: String::new(),
                            uint: 0,
                        },
                    },
                ],
                ..Default::default()
            },
        };
        let registry_programs = NativeRegistryPrograms {
            approval_program: vec![1, 2, 3],
            clear_state_program: vec![4, 5, 6],
        };

        verify_native_registry_programs(&app, &registry_programs).unwrap();
        assert_eq!(
            verify_native_registry_state(&app, &pool_programs, [42; 32]).unwrap(),
            0
        );
        assert!(verify_native_registry_state(&app, &pool_programs, [43; 32]).is_err());
    }

    #[test]
    fn rejects_native_registry_pool_hash_mismatch() {
        let app = ApplicationInfo {
            id: 7,
            params: opennodia_node::ApplicationParams {
                global_state: vec![
                    TealKeyValue {
                        key: b64(GLOBAL_KEY_REGISTRY_VERSION),
                        value: TealValue {
                            value_type: 2,
                            bytes: String::new(),
                            uint: 1,
                        },
                    },
                    TealKeyValue {
                        key: b64(GLOBAL_KEY_POOL_APPROVAL_HASH),
                        value: TealValue {
                            value_type: 1,
                            bytes: b64(&[0; 32]),
                            uint: 0,
                        },
                    },
                    TealKeyValue {
                        key: b64(GLOBAL_KEY_POOL_CLEAR_HASH),
                        value: TealValue {
                            value_type: 1,
                            bytes: b64(&[0; 32]),
                            uint: 0,
                        },
                    },
                    TealKeyValue {
                        key: b64(GLOBAL_KEY_REGISTRY_GENESIS_HASH),
                        value: TealValue {
                            value_type: 1,
                            bytes: b64(&[42; 32]),
                            uint: 0,
                        },
                    },
                    TealKeyValue {
                        key: b64(GLOBAL_KEY_REGISTRY_ACTIVE_COUNT),
                        value: TealValue {
                            value_type: 2,
                            bytes: String::new(),
                            uint: 0,
                        },
                    },
                ],
                ..Default::default()
            },
        };
        let pool_programs = NativePoolPrograms {
            approval_programs: vec![vec![7, 8, 9]],
            clear_state_program: vec![10, 11, 12],
        };

        assert!(verify_native_registry_state(&app, &pool_programs, [42; 32]).is_err());
    }
}
