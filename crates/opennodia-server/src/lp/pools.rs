use base64::Engine;
use opennodia_amm::{decode_pool_state, PoolKey, PoolState};
use opennodia_core::Address;
use opennodia_node::{AlgodClient, ApplicationInfo};

use crate::state::AppState;

use super::contracts::{
    compile_native_pool_programs, genesis_hash, pool_global_state, verify_native_pool_programs,
    NativePoolPrograms,
};
use super::{bad_request, service_unavailable, ApiResult, LpPositionResponse, PoolResponse};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativePoolLifecycle {
    Created,
    ReadyForBootstrap,
    Bootstrapped,
    InvalidPartial,
}

impl NativePoolLifecycle {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::ReadyForBootstrap => "ready_for_bootstrap",
            Self::Bootstrapped => "bootstrapped",
            Self::InvalidPartial => "invalid_partial_liquidity",
        }
    }

    const fn note(self) -> &'static str {
        match self {
            Self::Created => "pool app exists but setup has not created the LP asset yet",
            Self::ReadyForBootstrap => {
                "pool setup is complete and the first atomic bootstrap is required"
            }
            Self::Bootstrapped => "pool has active two-sided liquidity",
            Self::InvalidPartial => {
                "pool state is partial or inconsistent and is not treated as tradable"
            }
        }
    }

    const fn is_tradable(self) -> bool {
        matches!(self, Self::Bootstrapped)
    }
}

fn native_pool_lifecycle(pool: &PoolState) -> NativePoolLifecycle {
    match (
        pool.lp_asset_id,
        pool.reserve_0,
        pool.reserve_1,
        pool.total_lp_supply,
    ) {
        (0, 0, 0, 0) => NativePoolLifecycle::Created,
        (lp_asset_id, 0, 0, 0) if lp_asset_id != 0 => NativePoolLifecycle::ReadyForBootstrap,
        (lp_asset_id, reserve_0, reserve_1, total_lp_supply)
            if lp_asset_id != 0 && reserve_0 > 0 && reserve_1 > 0 && total_lp_supply > 0 =>
        {
            NativePoolLifecycle::Bootstrapped
        }
        _ => NativePoolLifecycle::InvalidPartial,
    }
}

pub(super) fn pool_is_tradable(pool: &PoolState) -> bool {
    native_pool_lifecycle(pool).is_tradable()
}

pub(super) fn ensure_pool_is_tradable(pool: &PoolState, context: &str) -> ApiResult<()> {
    let lifecycle = native_pool_lifecycle(pool);
    if lifecycle.is_tradable() {
        return Ok(());
    }
    Err(bad_request(format!(
        "{context}: pool is not tradable ({})",
        lifecycle.note()
    )))
}

pub(super) fn pool_response(pool: &PoolState) -> PoolResponse {
    let lifecycle = native_pool_lifecycle(pool);
    PoolResponse {
        pool_id: pool.key.id(),
        source: "native".into(),
        app_id: pool.app_id,
        app_address: Address::from_app_id(pool.app_id).to_string(),
        lp_asset_id: pool.lp_asset_id,
        asset_0: pool.key.asset_0,
        asset_1: pool.key.asset_1,
        fee_bps: pool.key.fee_bps,
        curve_id: pool.key.curve_id,
        contract_version: pool.key.contract_version,
        reserve_0: pool.reserve_0,
        reserve_1: pool.reserve_1,
        total_lp_supply: pool.total_lp_supply,
        source_round: pool.source_round,
        lifecycle: lifecycle.as_str().into(),
        tradable: lifecycle.is_tradable(),
        active_liquidity: lifecycle.is_tradable(),
        setup_required: matches!(lifecycle, NativePoolLifecycle::Created),
        bootstrap_required: matches!(lifecycle, NativePoolLifecycle::ReadyForBootstrap),
        status_note: lifecycle.note().into(),
    }
}

fn proportional_amount(balance: u64, reserve: u64, total_supply: u64) -> u64 {
    if total_supply == 0 {
        return 0;
    }
    let value = u128::from(balance) * u128::from(reserve) / u128::from(total_supply);
    value.min(u128::from(u64::MAX)) as u64
}

fn pool_share_ppm(balance: u64, total_supply: u64) -> u64 {
    if total_supply == 0 {
        return 0;
    }
    let value = u128::from(balance) * 1_000_000u128 / u128::from(total_supply);
    value.min(u128::from(u64::MAX)) as u64
}

pub(super) fn position_response(pool: &PoolState, lp_balance: u64) -> LpPositionResponse {
    LpPositionResponse {
        pool: pool_response(pool),
        lp_asset_id: pool.lp_asset_id,
        lp_balance,
        pool_share_ppm: pool_share_ppm(lp_balance, pool.total_lp_supply),
        underlying_0: proportional_amount(lp_balance, pool.reserve_0, pool.total_lp_supply),
        underlying_1: proportional_amount(lp_balance, pool.reserve_1, pool.total_lp_supply),
        cost_basis_available: false,
        pnl_available: false,
        note: "Cost basis and fee-vs-price PnL are not estimated without complete wallet history."
            .into(),
    }
}

pub(super) fn decode_registry_box_app_id(value: &str, registry_app_id: u64) -> ApiResult<u64> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|error| {
            bad_request(format!(
                "decode native AMM registry {registry_app_id} box value: {error}"
            ))
        })?;
    let bytes: [u8; 8] = decoded.try_into().map_err(|bytes: Vec<u8>| {
        bad_request(format!(
            "native AMM registry {registry_app_id} box value must be 8 bytes, got {}",
            bytes.len()
        ))
    })?;
    let app_id = u64::from_be_bytes(bytes);
    if app_id == 0 {
        return Err(bad_request(format!(
            "native AMM registry {registry_app_id} box value contains zero app id"
        )));
    }
    Ok(app_id)
}

async fn registry_pool_app_id(
    algod: &AlgodClient,
    registry_app_id: u64,
    pool_key: &PoolKey,
) -> ApiResult<Option<u64>> {
    let box_info = algod
        .application_box_by_name(registry_app_id, &pool_key.digest())
        .await
        .map_err(|error| {
            service_unavailable(format!(
                "fetch native AMM registry {registry_app_id} box: {error}"
            ))
        })?;
    box_info
        .map(|info| decode_registry_box_app_id(&info.value, registry_app_id))
        .transpose()
}

pub(super) async fn reject_registry_duplicate_pool(
    algod: &AlgodClient,
    registry_app_id: u64,
    pool_key: &PoolKey,
) -> ApiResult<()> {
    if let Some(app_id) = registry_pool_app_id(algod, registry_app_id, pool_key).await? {
        return Err(bad_request(format!(
            "pool {} is already registered in native AMM registry {} as application {}",
            pool_key.id(),
            registry_app_id,
            app_id
        )));
    }
    Ok(())
}

pub(super) fn decode_application_pool(
    app: &ApplicationInfo,
    genesis_hash: [u8; 32],
    source_round: u64,
    expected_programs: &NativePoolPrograms,
) -> ApiResult<PoolState> {
    verify_native_pool_programs(app, expected_programs)?;
    let global_state = pool_global_state(&app.params.global_state)?;
    decode_pool_state(app.id, genesis_hash, source_round, &global_state).map_err(|error| {
        bad_request(format!(
            "application {} is not a valid native pool: {error}",
            app.id
        ))
    })
}

pub(super) async fn read_pool(
    algod: &AlgodClient,
    app_id: u64,
    genesis_hash: [u8; 32],
    source_round: u64,
    expected_programs: &NativePoolPrograms,
) -> ApiResult<PoolState> {
    let app = algod
        .application_info(app_id)
        .await
        .map_err(|error| bad_request(format!("application {app_id} not found: {error}")))?;
    decode_application_pool(&app, genesis_hash, source_round, expected_programs)
}

pub(crate) async fn read_current_pool(algod: &AlgodClient, app_id: u64) -> ApiResult<PoolState> {
    let status = algod
        .status()
        .await
        .map_err(|error| service_unavailable(format!("fetch algod status: {error}")))?;
    let genesis_hash = genesis_hash(algod).await?;
    let expected_programs = compile_native_pool_programs(algod).await?;
    read_pool(
        algod,
        app_id,
        genesis_hash,
        status.last_round.as_u64(),
        &expected_programs,
    )
    .await
}

pub(crate) fn pool_execution_state_matches(current: &PoolState, prepared: &PoolState) -> bool {
    current.app_id == prepared.app_id
        && current.key == prepared.key
        && current.lp_asset_id == prepared.lp_asset_id
        && current.reserve_0 == prepared.reserve_0
        && current.reserve_1 == prepared.reserve_1
        && current.total_lp_supply == prepared.total_lp_supply
}

pub(super) async fn require_prepared_pool_state(
    algod: &AlgodClient,
    prepared: &PoolState,
    action: &str,
) -> ApiResult<PoolState> {
    let current = read_current_pool(algod, prepared.app_id).await?;
    if !pool_execution_state_matches(&current, prepared) {
        return Err(bad_request(format!(
            "pool state changed after {action} prepare; refresh and prepare again"
        )));
    }
    Ok(current)
}

pub(super) async fn record_pool_state(state: &AppState, pool: &PoolState) {
    let mut registry = state.stores.lp_registry.lock().await;
    if let Err(error) = registry.upsert(pool) {
        tracing::warn!(
            app_id = pool.app_id,
            pool_id = %pool.key.id(),
            %error,
            "failed to update LP pool registry"
        );
    }
}

pub(super) async fn reject_registered_duplicate_pool(
    state: &AppState,
    pool_key: &PoolKey,
) -> ApiResult<()> {
    let registry = state.stores.lp_registry.lock().await;
    if let Some(existing) = registry.find_pool_key(pool_key) {
        return Err(bad_request(format!(
            "native pool already exists for this pair, fee tier, and version: app {}",
            existing.app_id
        )));
    }
    Ok(())
}

pub(super) async fn reject_network_duplicate_pool(
    state: &AppState,
    algod: &AlgodClient,
    pool_key: &PoolKey,
    expected_programs: &NativePoolPrograms,
) -> ApiResult<()> {
    let Some(indexer) = state
        .ledger
        .indexer
        .as_ref()
        .or(state.ledger.public_indexer.as_ref())
    else {
        return Ok(());
    };

    let discover_asset = if pool_key.asset_0 == 0 {
        pool_key.asset_1
    } else {
        pool_key.asset_0
    };
    let applications = indexer
        .applications_by_asset(discover_asset)
        .await
        .map_err(|error| {
            service_unavailable(format!("pool duplicate discovery failed: {error}"))
        })?;
    for app in applications {
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
                        "skipping duplicate candidate that could not be loaded from algod"
                    );
                    continue;
                }
            };
        }
        let Ok(pool) =
            decode_application_pool(&app_info, pool_key.genesis_hash, 0, expected_programs)
        else {
            continue;
        };
        if pool.key == *pool_key {
            return Err(bad_request(format!(
                "native pool already exists on-chain for this pair, fee tier, and version: app {}",
                pool.app_id
            )));
        }
    }
    Ok(())
}
