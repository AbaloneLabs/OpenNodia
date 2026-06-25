use std::time::{SystemTime, UNIX_EPOCH};

use opennodia_core::Address;
use opennodia_node::AlgodClient;

use super::sources::ExternalManifest;
use super::{
    api_error, available_algo, bad_request, fetch_account, internal, mul_div_floor,
    optional_state_bytes_raw_key, service_unavailable, state_bytes, teal_state_map,
    uint_from_be_bytes, ApiResult, FolksBackedInfo,
};

#[derive(Debug, Clone)]
struct FolksPoolRef {
    pool_app_id: u64,
    underlying_asset_id: u64,
    f_asset_id: u64,
    deposit_interest_rate: u64,
    deposit_interest_index: u64,
    redeem_available: u64,
    f_asset_outstanding: u64,
    total_deposit: u64,
    total_borrowed: u64,
    utilization_bps: u64,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct FolksManagerPoolInfo {
    pub(super) pool_app_id: u64,
    pub(super) deposit_interest_rate: u64,
    pub(super) deposit_interest_index: u64,
    pub(super) updated_at: u64,
}

pub(super) fn current_deposit_interest_index(
    deposit_interest_rate: u64,
    base_deposit_interest_index: u64,
    updated_at: u64,
    now: u64,
) -> ApiResult<u64> {
    if base_deposit_interest_index == 0 || now <= updated_at {
        return Ok(base_deposit_interest_index);
    }
    let elapsed = now - updated_at;
    let scale = 10_000_000_000_000_000u128;
    let seconds_in_year = 31_536_000u128;
    let growth = scale
        .checked_add(
            u128::from(deposit_interest_rate)
                .checked_mul(u128::from(elapsed))
                .ok_or_else(|| bad_request("Folks deposit interest calculation overflow"))?
                / seconds_in_year,
        )
        .ok_or_else(|| bad_request("Folks deposit interest calculation overflow"))?;
    let current = u128::from(base_deposit_interest_index)
        .checked_mul(growth)
        .ok_or_else(|| bad_request("Folks deposit interest index overflow"))?
        / scale;
    u64::try_from(current).map_err(|_| bad_request("Folks deposit interest index overflow"))
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

pub(super) async fn folks_backed_info_for_pact_pool(
    algod: &AlgodClient,
    manifest: ExternalManifest,
    asset_0: u64,
    asset_1: u64,
) -> ApiResult<Option<FolksBackedInfo>> {
    let Some(manager_app_id) = manifest.folks_pool_manager_app_id else {
        return Ok(None);
    };
    let Some(adapter_app_id) = manifest.pact_folks_lending_pool_adapter_app_id else {
        return Ok(None);
    };
    let app = algod
        .application_info(manager_app_id)
        .await
        .map_err(|error| service_unavailable(format!("fetch Folks pool manager app: {error}")))?;
    let state = teal_state_map(&app.params.global_state)?;
    let mut data = Vec::with_capacity(63 * 126);
    for key in 0u8..63 {
        let Some(bytes) = optional_state_bytes_raw_key(&state, &[key]) else {
            return Ok(None);
        };
        data.extend_from_slice(&bytes);
    }

    let mut first = None;
    let mut second = None;
    for chunk in data.chunks_exact(42).take(186) {
        let manager_pool = parse_folks_manager_pool_info(chunk)?;
        if manager_pool.pool_app_id == 0 {
            continue;
        }
        let pool = match read_folks_lending_pool(algod, manager_pool).await {
            Ok(pool) => pool,
            Err(error) => {
                tracing::debug!(
                    pool_app_id = manager_pool.pool_app_id,
                    error = api_error(&error),
                    "skipping Folks lending pool that could not be verified"
                );
                continue;
            }
        };
        if pool.f_asset_id == asset_0 {
            first = Some(pool);
        } else if pool.f_asset_id == asset_1 {
            second = Some(pool);
        }
        if first.is_some() && second.is_some() {
            break;
        }
    }

    let (Some(pool_0), Some(pool_1)) = (first, second) else {
        return Ok(None);
    };
    Ok(Some(FolksBackedInfo {
        source: "Folks Finance lending-backed Pact pool".into(),
        adapter_app_id: Some(adapter_app_id),
        pool_0_app_id: pool_0.pool_app_id,
        pool_1_app_id: pool_1.pool_app_id,
        underlying_0: pool_0.underlying_asset_id,
        underlying_1: pool_1.underlying_asset_id,
        f_asset_0: pool_0.f_asset_id,
        f_asset_1: pool_1.f_asset_id,
        deposit_interest_rate_0: pool_0.deposit_interest_rate,
        deposit_interest_rate_1: pool_1.deposit_interest_rate,
        deposit_interest_index_0: pool_0.deposit_interest_index,
        deposit_interest_index_1: pool_1.deposit_interest_index,
        redeem_available_0: pool_0.redeem_available,
        redeem_available_1: pool_1.redeem_available,
        f_asset_outstanding_0: pool_0.f_asset_outstanding,
        f_asset_outstanding_1: pool_1.f_asset_outstanding,
        total_deposit_0: pool_0.total_deposit,
        total_deposit_1: pool_1.total_deposit,
        total_borrowed_0: pool_0.total_borrowed,
        total_borrowed_1: pool_1.total_borrowed,
        utilization_bps_0: pool_0.utilization_bps,
        utilization_bps_1: pool_1.utilization_bps,
        utilization_available: true,
        utilization_note:
            "redeemable liquidity is verified from lending pool escrow holdings; utilization is computed from outstanding fAsset supply converted by the current deposit interest index"
                .into(),
        risk_note:
            "liquidity is still executed through the underlying Pact pool; Folks adapter composition risk is shown separately and is not counted as an additional AMM source"
                .into(),
    }))
}

pub(super) fn parse_folks_manager_pool_info(chunk: &[u8]) -> ApiResult<FolksManagerPoolInfo> {
    if chunk.len() != 42 {
        return Err(internal(format!(
            "Folks manager pool record must be 42 bytes, got {}",
            chunk.len()
        )));
    }
    Ok(FolksManagerPoolInfo {
        pool_app_id: uint_from_be_bytes(&chunk[0..6]),
        deposit_interest_rate: uint_from_be_bytes(&chunk[22..30]),
        deposit_interest_index: uint_from_be_bytes(&chunk[30..38]),
        updated_at: uint_from_be_bytes(&chunk[38..42]),
    })
}

async fn read_folks_lending_pool(
    algod: &AlgodClient,
    manager_pool: FolksManagerPoolInfo,
) -> ApiResult<FolksPoolRef> {
    let pool_app_id = manager_pool.pool_app_id;
    let app = algod
        .application_info(pool_app_id)
        .await
        .map_err(|error| service_unavailable(format!("fetch Folks lending pool app: {error}")))?;
    let state = teal_state_map(&app.params.global_state)?;
    let assets = state_bytes(&state, "a")?;
    if assets.len() < 16 {
        return Err(bad_request("Folks lending pool asset state is too short"));
    }
    let interest = state_bytes(&state, "i")?;
    if interest.len() < 56 {
        return Err(bad_request(
            "Folks lending pool interest state is too short",
        ));
    }
    let underlying_asset_id = uint_from_be_bytes(&assets[0..8]);
    let f_asset_id = uint_from_be_bytes(&assets[8..16]);
    let app_deposit_interest_rate = uint_from_be_bytes(&interest[32..40]);
    let app_deposit_interest_index = uint_from_be_bytes(&interest[40..48]);
    let app_updated_at = uint_from_be_bytes(&interest[48..56]);
    let deposit_interest_rate = if manager_pool.deposit_interest_rate != 0 {
        manager_pool.deposit_interest_rate
    } else {
        app_deposit_interest_rate
    };
    let base_deposit_interest_index = if manager_pool.deposit_interest_index != 0 {
        manager_pool.deposit_interest_index
    } else {
        app_deposit_interest_index
    };
    let updated_at = if manager_pool.updated_at != 0 {
        manager_pool.updated_at
    } else {
        app_updated_at
    };
    let deposit_interest_index = current_deposit_interest_index(
        deposit_interest_rate,
        base_deposit_interest_index,
        updated_at,
        unix_timestamp(),
    )?;
    let app_address = Address::from_app_id(pool_app_id);
    let redeem_available = read_asset_available(algod, app_address, underlying_asset_id).await?;
    let f_asset_params = algod
        .asset_params(f_asset_id)
        .await
        .map_err(|error| service_unavailable(format!("fetch Folks fAsset params: {error}")))?;
    let f_asset_held_by_pool = read_asset_available(algod, app_address, f_asset_id).await?;
    let f_asset_outstanding = f_asset_params.total.saturating_sub(f_asset_held_by_pool);
    let total_deposit = mul_div_floor(
        f_asset_outstanding,
        deposit_interest_index,
        100_000_000_000_000,
    )?;
    let total_borrowed = total_deposit.saturating_sub(redeem_available);
    let utilization_bps = if total_deposit == 0 {
        0
    } else {
        mul_div_floor(total_borrowed, 10_000, total_deposit)?
    };
    Ok(FolksPoolRef {
        pool_app_id,
        underlying_asset_id,
        f_asset_id,
        deposit_interest_rate,
        deposit_interest_index,
        redeem_available,
        f_asset_outstanding,
        total_deposit,
        total_borrowed,
        utilization_bps,
    })
}

async fn read_asset_available(
    algod: &AlgodClient,
    address: Address,
    asset_id: u64,
) -> ApiResult<u64> {
    let account = fetch_account(algod, address).await?;
    if asset_id == 0 {
        return Ok(available_algo(&account));
    }
    Ok(account
        .assets
        .iter()
        .find(|holding| holding.asset_id == asset_id && !holding.is_frozen)
        .map(|holding| holding.amount)
        .unwrap_or(0))
}
