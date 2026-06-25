//! Native LP safety guards.

use axum::Json;
use opennodia_core::{Address, Network};

use crate::routes::ApiError;
use crate::state::AppState;

use super::{service_unavailable, ApiResult};

pub(super) fn authority_is_enabled(value: &str) -> ApiResult<bool> {
    if value.is_empty() {
        return Ok(false);
    }
    let address: Address = value.parse().map_err(|error| {
        service_unavailable(format!("invalid asset authority address: {error}"))
    })?;
    Ok(!address.is_zero())
}

pub(super) fn native_amm_writes_allowed_for(
    network: Network,
    mainnet_write_enabled_after_audit: bool,
) -> bool {
    network != Network::Mainnet || mainnet_write_enabled_after_audit
}

pub(super) fn native_amm_writes_allowed(state: &AppState) -> bool {
    native_amm_writes_allowed_for(
        state.config.algod.network,
        state.config.lp.mainnet_write_enabled_after_audit,
    )
}

pub(super) fn native_amm_write_status_note(state: &AppState) -> String {
    if native_amm_writes_allowed(state) {
        "native AMM writes are enabled for this network/configuration".into()
    } else {
        "mainnet native AMM writes are fail-closed until independent audit opt-in is enabled".into()
    }
}

pub(super) fn ensure_native_amm_writes_allowed(state: &AppState) -> ApiResult<()> {
    if native_amm_writes_allowed(state) {
        return Ok(());
    }
    Err((
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        Json(ApiError::new(native_amm_write_status_note(state))),
    ))
}
