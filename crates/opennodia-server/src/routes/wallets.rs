use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::api_error::ApiError;
use crate::state::AppState;
use crate::wallet::{RegisteredWallet, WalletSource};

use super::verify_pin;

pub(super) fn wallet_routes() -> Router<AppState> {
    Router::new()
        .route("/api/wallets", get(list_wallets))
        .route("/api/wallets/create", post(create_wallet))
        .route("/api/wallets/import", post(import_wallet))
        .route("/api/wallets/activate", post(activate_wallet))
        .route("/api/wallets/active", get(active_wallet))
        .route("/api/wallets/{id}/addresses", post(list_addresses))
        .route("/api/wallets/{id}/address", post(generate_address))
        .route(
            "/api/wallets/{id}",
            axum::routing::patch(rename_wallet).delete(remove_wallet),
        )
}

#[derive(Debug, Deserialize)]
struct CreateWalletRequest {
    name: String,
    pin: String,
}

#[derive(Debug, Deserialize)]
struct ImportWalletRequest {
    name: String,
    mnemonic: String,
    pin: String,
}

#[derive(Debug, Deserialize)]
struct ActivateWalletRequest {
    wallet_id: String,
}

#[derive(Debug, Deserialize)]
struct RenameWalletRequest {
    name: String,
}

#[derive(Debug, Deserialize)]
struct GenerateAddressRequest {
    pin: String,
}

#[derive(Debug, Serialize)]
struct WalletResponse {
    id: String,
    name: String,
    source: String,
    first_address: String,
}

impl From<RegisteredWallet> for WalletResponse {
    fn from(w: RegisteredWallet) -> Self {
        Self {
            id: w.id,
            name: w.name,
            source: match w.source {
                WalletSource::Kmd => "kmd".into(),
                WalletSource::Imported => "imported".into(),
            },
            first_address: w.first_address,
        }
    }
}

#[derive(Debug, Serialize)]
struct ActiveWalletResponse {
    wallet: Option<WalletResponse>,
    addresses: Vec<String>,
}

/// `GET /api/wallets` — list all registered wallets.
async fn list_wallets(State(state): State<AppState>) -> Json<Vec<WalletResponse>> {
    let wallets = state.stores.wallets.list_wallets().await;
    Json(wallets.into_iter().map(WalletResponse::from).collect())
}

/// `POST /api/wallets/create` — create a new kmd wallet.
async fn create_wallet(
    State(state): State<AppState>,
    Json(req): Json<CreateWalletRequest>,
) -> Result<(StatusCode, Json<WalletResponse>), (StatusCode, Json<ApiError>)> {
    let pin = verify_pin(&state, &req.pin).await?;

    let wallet = state
        .stores
        .wallets
        .create_wallet(&req.name, pin.as_str())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new(format!("create wallet: {e}"))),
            )
        })?;

    let sync_state = state.clone();
    let sync_address = wallet.first_address.clone();
    tokio::spawn(async move {
        if let Err(error) = sync_state.sync_wallet_history_address(&sync_address).await {
            tracing::warn!(address = %sync_address, %error, "new wallet history sync failed");
        }
    });

    Ok((StatusCode::CREATED, Json(WalletResponse::from(wallet))))
}

/// `POST /api/wallets/import` — import a wallet from a 25-word mnemonic.
async fn import_wallet(
    State(state): State<AppState>,
    Json(req): Json<ImportWalletRequest>,
) -> Result<(StatusCode, Json<WalletResponse>), (StatusCode, Json<ApiError>)> {
    let pin = verify_pin(&state, &req.pin).await?;

    let wallet = state
        .stores
        .wallets
        .import_wallet(&req.name, &req.mnemonic, pin.as_str())
        .await
        .map_err(|e| {
            let code = if e.to_string().contains("mnemonic") {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (code, Json(ApiError::new(format!("import wallet: {e}"))))
        })?;

    let sync_state = state.clone();
    let sync_address = wallet.first_address.clone();
    tokio::spawn(async move {
        if let Err(error) = sync_state.sync_wallet_history_address(&sync_address).await {
            tracing::warn!(address = %sync_address, %error, "imported wallet history sync failed");
        }
    });

    Ok((StatusCode::CREATED, Json(WalletResponse::from(wallet))))
}

/// `POST /api/wallets/activate` — set the active wallet.
async fn activate_wallet(
    State(state): State<AppState>,
    Json(req): Json<ActivateWalletRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    state
        .stores
        .wallets
        .activate(&req.wallet_id)
        .await
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(ApiError::new(format!("activate wallet: {e}"))),
            )
        })?;
    Ok(StatusCode::OK)
}

/// `PATCH /api/wallets/:id` — rename a wallet.
async fn rename_wallet(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<RenameWalletRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    state
        .stores
        .wallets
        .rename_wallet(&id, &req.name)
        .await
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(ApiError::new(format!("rename wallet: {e}"))),
            )
        })?;
    Ok(StatusCode::OK)
}

/// `GET /api/wallets/active` — get active wallet + its addresses.
async fn active_wallet(
    State(state): State<AppState>,
) -> Result<Json<ActiveWalletResponse>, (StatusCode, Json<ApiError>)> {
    let wallet = state.stores.wallets.active_wallet().await;
    let wallet_resp = wallet.map(WalletResponse::from);

    Ok(Json(ActiveWalletResponse {
        wallet: wallet_resp,
        addresses: vec![],
    }))
}

/// `POST /api/wallets/:id/addresses` — list addresses in a wallet.
async fn list_addresses(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<GenerateAddressRequest>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<ApiError>)> {
    let pin = verify_pin(&state, &req.pin).await?;
    let addresses = state
        .stores
        .wallets
        .list_addresses(&id, pin.as_str())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new(format!("list addresses: {e}"))),
            )
        })?;
    Ok(Json(addresses))
}

/// `POST /api/wallets/:id/address` — generate a new address in a wallet.
async fn generate_address(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<GenerateAddressRequest>,
) -> Result<Json<String>, (StatusCode, Json<ApiError>)> {
    let pin = verify_pin(&state, &req.pin).await?;
    let address = state
        .stores
        .wallets
        .generate_address(&id, pin.as_str())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new(format!("generate address: {e}"))),
            )
        })?;

    let sync_state = state.clone();
    let sync_address = address.clone();
    tokio::spawn(async move {
        if let Err(error) = sync_state.sync_wallet_history_address(&sync_address).await {
            tracing::warn!(address = %sync_address, %error, "generated address history sync failed");
        }
    });

    Ok(Json(address))
}

/// `DELETE /api/wallets/:id` — remove a wallet from the registry.
async fn remove_wallet(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let addresses = state
        .stores
        .wallets
        .tracked_wallet_addresses(&id)
        .await
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(ApiError::new(format!("remove wallet: {e}"))),
            )
        })?;

    if let Some(store) = state.stores.wallet_history.as_ref() {
        for address in &addresses {
            if state
                .stores
                .wallets
                .address_registered_elsewhere(&id, address)
                .await
            {
                continue;
            }
            store.delete_address(address).await.map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError::new(format!("remove wallet history: {e}"))),
                )
            })?;
        }
    }

    if let Some(store) = state.stores.asset_metadata.as_ref() {
        let network = state.config.algod.network.to_string();
        for address in &addresses {
            if state
                .stores
                .wallets
                .address_registered_elsewhere(&id, address)
                .await
            {
                continue;
            }
            store.delete_address(&network, address).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError::new(format!("remove asset metadata: {e}"))),
                )
            })?;
        }
    }

    state.stores.wallets.remove(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(format!("remove wallet: {e}"))),
        )
    })?;
    Ok(StatusCode::OK)
}
