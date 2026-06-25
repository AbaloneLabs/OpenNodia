use opennodia_core::Address;
use opennodia_swap::EscrowAccount;

use crate::state::AppState;

use super::{bad_request, service_unavailable, ApiResult};

pub(super) async fn reject_regulated_asset(state: &AppState, asset_id: u64) -> ApiResult<()> {
    let params = fetch_asset_params(state, asset_id).await?;
    let grade = opennodia_assets::AssetPolicyGrade::classify(
        authority_is_enabled(&params.freeze)?,
        authority_is_enabled(&params.clawback)?,
        params.default_frozen,
    );
    if matches!(grade, opennodia_assets::AssetPolicyGrade::Regulated) {
        return Err(bad_request(format!(
            "asset {asset_id} is regulated (freeze/clawback) and cannot be traded on the DEX"
        )));
    }
    Ok(())
}

pub(super) async fn reject_escrow_regulated_assets(
    state: &AppState,
    escrow: &EscrowAccount,
) -> ApiResult<()> {
    if escrow.params.sell_asset != 0 {
        reject_regulated_asset(state, escrow.params.sell_asset).await?;
    }
    if escrow.params.buy_asset != 0 {
        reject_regulated_asset(state, escrow.params.buy_asset).await?;
    }
    Ok(())
}

fn authority_is_enabled(value: &str) -> ApiResult<bool> {
    if value.is_empty() {
        return Ok(false);
    }
    let address: Address = value.parse().map_err(|error| {
        service_unavailable(format!("invalid asset authority address: {error}"))
    })?;
    Ok(!address.is_zero())
}

async fn fetch_asset_params(
    state: &AppState,
    asset_id: u64,
) -> ApiResult<opennodia_node::AssetParams> {
    const TTL: std::time::Duration = std::time::Duration::from_secs(300);
    {
        let cache = state.caches.asset_params.lock().await;
        if let Some((fetched_at, params)) = cache.get(&asset_id) {
            if fetched_at.elapsed() < TTL {
                return Ok(params.clone());
            }
        }
    }
    let (params, _) = state
        .ledger
        .algod
        .asset_params_with_fallback(state.ledger.public_algod.as_ref(), asset_id)
        .await
        .map_err(|error| {
            service_unavailable(format!("fetch asset {asset_id} parameters: {error}"))
        })?;
    {
        let mut cache = state.caches.asset_params.lock().await;
        cache.insert(asset_id, (std::time::Instant::now(), params.clone()));
    }
    Ok(params)
}
