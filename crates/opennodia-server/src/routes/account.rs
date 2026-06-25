use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use opennodia_core::Address;
use opennodia_node::DataSource;
use serde::{Deserialize, Serialize};

use crate::api_error::ApiError;
use crate::asset_metadata::{AssetMetadataUpdate, AssetUserMetadata};
use crate::state::AppState;
use crate::wallet_history::BalanceSnapshotAsset;

/// TTL for the account info cache when using public API fallback.
const ACCOUNT_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(30);

/// TTL for the asset params cache.
const ASSET_PARAMS_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(300);

pub(super) fn account_routes() -> Router<AppState> {
    Router::new()
        .route("/api/accounts/{addr}", get(account_info))
        .route("/api/accounts/{addr}/assets", get(account_assets))
        .route(
            "/api/accounts/{addr}/asset-metadata",
            get(list_asset_metadata),
        )
        .route(
            "/api/accounts/{addr}/assets/{asset_id}/metadata",
            put(update_asset_metadata).delete(clear_asset_metadata),
        )
        .route("/api/assets/search", get(search_assets))
        .route("/api/assets/{id}", get(asset_metadata))
}

#[derive(Debug, Serialize)]
struct AccountInfoResponse {
    #[serde(flatten)]
    account: opennodia_node::AccountInfo,
    source: DataSource,
    cached: bool,
}

#[derive(Debug, Serialize)]
struct AssetEntry {
    kind: &'static str,
    id: u64,
    name: String,
    unit: String,
    decimals: u32,
    amount: u64,
    policy: &'static str,
    frozen: bool,
}

#[derive(Debug, Serialize)]
struct AccountAssetsResponse {
    address: String,
    assets: Vec<AssetEntry>,
    round: u64,
    source: DataSource,
}

#[derive(Debug, Serialize)]
struct AssetMetadataListResponse {
    network: String,
    address: String,
    metadata: Vec<AssetUserMetadata>,
}

#[derive(Debug, Deserialize)]
struct AssetMetadataUpdateRequest {
    #[serde(default)]
    tag: String,
    #[serde(default)]
    memo: String,
    #[serde(default)]
    color_label: String,
    #[serde(default)]
    pinned: bool,
}

impl From<AssetMetadataUpdateRequest> for AssetMetadataUpdate {
    fn from(value: AssetMetadataUpdateRequest) -> Self {
        Self {
            tag: value.tag,
            memo: value.memo,
            color_label: value.color_label,
            pinned: value.pinned,
        }
    }
}

#[derive(Debug, Deserialize)]
struct SearchAssetsQuery {
    q: String,
}

#[derive(Debug, Serialize)]
struct AssetSearchEntry {
    id: u64,
    name: String,
    unit: String,
    decimals: u32,
    total: u64,
    creator: String,
    verified: bool,
}

#[derive(Debug, Serialize)]
struct AssetMetadataResponse {
    id: u64,
    name: String,
    unit: String,
    decimals: u32,
    total: u64,
    creator: String,
    url: String,
    manager: String,
    reserve: String,
    freeze: String,
    clawback: String,
    default_frozen: bool,
    source: String,
}

/// `GET /api/accounts/:addr` — account information with public fallback.
async fn account_info(
    State(state): State<AppState>,
    Path(addr): Path<String>,
) -> Result<Json<AccountInfoResponse>, (StatusCode, Json<ApiError>)> {
    let cached = {
        let cache = state.caches.account_info.lock().await;
        if let Some((fetched_at, info)) = cache.get(&addr) {
            if fetched_at.elapsed() < ACCOUNT_CACHE_TTL {
                Some(info.clone())
            } else {
                None
            }
        } else {
            None
        }
    };
    if let Some(info) = cached {
        return Ok(Json(AccountInfoResponse {
            account: info,
            source: DataSource::Public,
            cached: true,
        }));
    }

    let (info, source) = fetch_account(&state, &addr).await.map_err(|e| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new(format!("account lookup failed: {e}"))),
        )
    })?;

    Ok(Json(AccountInfoResponse {
        account: info,
        source,
        cached: false,
    }))
}

/// `GET /api/accounts/:addr/assets` — unified asset list for an account.
async fn account_assets(
    State(state): State<AppState>,
    Path(addr): Path<String>,
) -> Result<Json<AccountAssetsResponse>, (StatusCode, Json<ApiError>)> {
    let (info, source) = fetch_account(&state, &addr).await.map_err(|e| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new(format!("account lookup failed: {e}"))),
        )
    })?;

    let mut entries = Vec::with_capacity(info.assets.len() + 1);
    entries.push(AssetEntry {
        kind: "native",
        id: 0,
        name: "Algo".into(),
        unit: "ALGO".into(),
        decimals: 6,
        amount: info.amount,
        policy: "native",
        frozen: false,
    });

    for holding in &info.assets {
        let params = fetch_asset_params(&state, holding.asset_id)
            .await
            .map_err(|error| {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(ApiError::new(format!(
                        "asset {} lookup failed: {error}",
                        holding.asset_id
                    ))),
                )
            })?;
        let grade = opennodia_assets::AssetPolicyGrade::classify(
            asset_authority_enabled(&params.freeze).map_err(|error| {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(ApiError::new(error.to_string())),
                )
            })?,
            asset_authority_enabled(&params.clawback).map_err(|error| {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(ApiError::new(error.to_string())),
                )
            })?,
            params.default_frozen,
        );
        let policy = match grade {
            opennodia_assets::AssetPolicyGrade::Open => "open",
            opennodia_assets::AssetPolicyGrade::Bridged => "bridged",
            opennodia_assets::AssetPolicyGrade::Regulated => "regulated",
        };
        entries.push(AssetEntry {
            kind: "asa",
            id: holding.asset_id,
            name: if params.name.is_empty() {
                format!("Asset #{}", holding.asset_id)
            } else {
                params.name.clone()
            },
            unit: if params.unit_name.is_empty() {
                format!("#{}", holding.asset_id)
            } else {
                params.unit_name.clone()
            },
            decimals: params.decimals,
            amount: holding.amount,
            policy,
            frozen: holding.is_frozen,
        });
    }

    let snapshot_assets = entries
        .iter()
        .map(|entry| BalanceSnapshotAsset {
            asset_id: entry.id,
            kind: entry.kind.to_string(),
            name: entry.name.clone(),
            unit: entry.unit.clone(),
            decimals: entry.decimals,
            amount: entry.amount,
        })
        .collect::<Vec<_>>();
    let snapshot_state = state.clone();
    let snapshot_address = addr.clone();
    let source_round = info.round;
    tokio::spawn(async move {
        if let Err(error) = snapshot_state
            .record_balance_snapshot(&snapshot_address, source_round, &snapshot_assets)
            .await
        {
            tracing::warn!(
                address = %snapshot_address,
                %error,
                "monthly balance snapshot update failed"
            );
        }
    });

    Ok(Json(AccountAssetsResponse {
        address: addr,
        assets: entries,
        round: info.round,
        source,
    }))
}

/// `GET /api/accounts/:addr/asset-metadata` — local user labels.
async fn list_asset_metadata(
    State(state): State<AppState>,
    Path(addr): Path<String>,
) -> Result<Json<AssetMetadataListResponse>, (StatusCode, Json<ApiError>)> {
    let address = require_registered_metadata_address(&state, &addr).await?;
    let store = asset_metadata_store(&state)?;
    let network = state.config.algod.network.to_string();
    let metadata = store.list(&network, &address).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(format!("list asset metadata: {e}"))),
        )
    })?;
    Ok(Json(AssetMetadataListResponse {
        network,
        address,
        metadata,
    }))
}

/// `PUT /api/accounts/:addr/assets/:asset_id/metadata` — save local user labels.
async fn update_asset_metadata(
    State(state): State<AppState>,
    Path((addr, asset_id)): Path<(String, u64)>,
    Json(req): Json<AssetMetadataUpdateRequest>,
) -> Result<Json<AssetUserMetadata>, (StatusCode, Json<ApiError>)> {
    let address = require_registered_metadata_address(&state, &addr).await?;
    let store = asset_metadata_store(&state)?;
    let network = state.config.algod.network.to_string();
    let record = store
        .upsert(&network, &address, asset_id, req.into())
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError::new(format!("save asset metadata: {e}"))),
            )
        })?;
    Ok(Json(record))
}

/// `DELETE /api/accounts/:addr/assets/:asset_id/metadata` — remove local user labels.
async fn clear_asset_metadata(
    State(state): State<AppState>,
    Path((addr, asset_id)): Path<(String, u64)>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let address = require_registered_metadata_address(&state, &addr).await?;
    let store = asset_metadata_store(&state)?;
    let network = state.config.algod.network.to_string();
    store.clear(&network, &address, asset_id).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(format!("clear asset metadata: {e}"))),
        )
    })?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/assets/search?q=...` — search assets by name or unit via indexer.
async fn search_assets(
    State(state): State<AppState>,
    Query(query): Query<SearchAssetsQuery>,
) -> Result<Json<Vec<AssetSearchEntry>>, (StatusCode, Json<ApiError>)> {
    let query_trimmed = query.q.trim();
    if query_trimmed.len() < 2 {
        return Ok(Json(vec![]));
    }

    let Some((indexer, _source)) = state.effective_search_client().await else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new(
                "Indexer is not configured. Set use_public_fallback = true in opennodia.toml.",
            )),
        ));
    };

    let resp = indexer.search_assets(query_trimmed).await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(ApiError::new(format!("indexer search failed: {e}"))),
        )
    })?;

    let entries = resp
        .assets
        .into_iter()
        .filter(|a| a.destroyed_at_round == 0)
        .map(|a| AssetSearchEntry {
            id: a.index,
            name: if a.params.name.is_empty() {
                format!("Asset #{}", a.index)
            } else {
                a.params.name.clone()
            },
            unit: if a.params.unit_name.is_empty() {
                format!("#{}", a.index)
            } else {
                a.params.unit_name.clone()
            },
            decimals: a.params.decimals,
            total: a.params.total,
            creator: a.params.creator.clone(),
            verified: false,
        })
        .collect();

    Ok(Json(entries))
}

/// `GET /api/assets/:id` — fetch full asset metadata via indexer or algod fallback.
async fn asset_metadata(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<AssetMetadataResponse>, (StatusCode, Json<ApiError>)> {
    if let Some((indexer, _source)) = state.effective_search_client().await {
        if let Ok(resp) = indexer.search_assets(&id.to_string()).await {
            if let Some(found) = resp.assets.into_iter().find(|a| a.index == id) {
                return Ok(Json(AssetMetadataResponse {
                    id,
                    name: found.params.name,
                    unit: found.params.unit_name,
                    decimals: found.params.decimals,
                    total: found.params.total,
                    creator: found.params.creator,
                    url: found.params.url,
                    manager: found.params.manager,
                    reserve: found.params.reserve,
                    freeze: found.params.freeze,
                    clawback: found.params.clawback,
                    default_frozen: found.params.default_frozen,
                    source: "indexer".to_string(),
                }));
            }
        }
    }

    let params = fetch_asset_params(&state, id).await.map_err(|error| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new(format!("asset {id} lookup failed: {error}"))),
        )
    })?;
    Ok(Json(AssetMetadataResponse {
        id,
        name: params.name,
        unit: params.unit_name,
        decimals: params.decimals,
        total: params.total,
        creator: params.creator,
        url: params.url,
        manager: params.manager,
        reserve: params.reserve,
        freeze: params.freeze,
        clawback: params.clawback,
        default_frozen: params.default_frozen,
        source: "algod".to_string(),
    }))
}

fn asset_metadata_store(
    state: &AppState,
) -> Result<&std::sync::Arc<crate::asset_metadata::AssetMetadataStore>, (StatusCode, Json<ApiError>)>
{
    state.stores.asset_metadata.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new("asset metadata database unavailable")),
        )
    })
}

pub(super) async fn require_registered_metadata_address(
    state: &AppState,
    addr: &str,
) -> Result<String, (StatusCode, Json<ApiError>)> {
    let address = addr.parse::<Address>().map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(format!("invalid account address: {e}"))),
        )
    })?;
    let normalized = address.to_string();
    if !state
        .stores
        .wallets
        .contains_registered_address(&normalized)
        .await
    {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ApiError::new(
                "asset metadata is only available for registered wallet addresses",
            )),
        ));
    }
    Ok(normalized)
}

pub(super) async fn fetch_account(
    state: &AppState,
    addr: &str,
) -> Result<(opennodia_node::AccountInfo, DataSource), anyhow::Error> {
    {
        let cache = state.caches.account_info.lock().await;
        if let Some((fetched_at, info)) = cache.get(addr) {
            if fetched_at.elapsed() < ACCOUNT_CACHE_TTL {
                return Ok((info.clone(), DataSource::Public));
            }
        }
    }

    let (info, source) = state
        .ledger
        .algod
        .account_info_with_fallback(state.ledger.public_algod.as_ref(), addr)
        .await?;

    if source == DataSource::Public {
        let mut cache = state.caches.account_info.lock().await;
        cache.insert(addr.to_string(), (std::time::Instant::now(), info.clone()));
    }
    Ok((info, source))
}

pub(super) async fn fetch_asset_params(
    state: &AppState,
    asset_id: u64,
) -> Result<opennodia_node::AssetParams, anyhow::Error> {
    {
        let cache = state.caches.asset_params.lock().await;
        if let Some((fetched_at, params)) = cache.get(&asset_id) {
            if fetched_at.elapsed() < ASSET_PARAMS_CACHE_TTL {
                return Ok(params.clone());
            }
        }
    }

    let (params, _) = state
        .ledger
        .algod
        .asset_params_with_fallback(state.ledger.public_algod.as_ref(), asset_id)
        .await?;

    {
        let mut cache = state.caches.asset_params.lock().await;
        cache.insert(asset_id, (std::time::Instant::now(), params.clone()));
    }

    Ok(params)
}

pub(super) fn asset_authority_enabled(value: &str) -> Result<bool, anyhow::Error> {
    if value.is_empty() {
        return Ok(false);
    }
    let address: Address = value.parse()?;
    Ok(!address.is_zero())
}
