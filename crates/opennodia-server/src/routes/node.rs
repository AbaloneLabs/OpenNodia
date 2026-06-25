use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use opennodia_node::DataSource;
use serde::Serialize;

use crate::api_error::ApiError;
use crate::state::AppState;

pub(super) fn node_routes() -> Router<AppState> {
    Router::new()
        .route("/api/node/status", get(node_status))
        .route("/api/node/sync-progress", get(sync_progress))
        .route("/api/node/block-info", get(block_info))
        .route("/api/node/participation-stats", get(participation_stats))
        .route("/api/indexer/status", get(indexer_status))
        .route("/api/indexer/sync-progress", get(indexer_sync_progress))
}

#[derive(Debug, Serialize)]
struct NodeStatusResponse {
    last_round: u64,
    #[serde(default)]
    last_version: String,
    #[serde(default)]
    time_since_last_round: u64,
    #[serde(default)]
    catchup_time: u64,
    source: DataSource,
}

#[derive(Debug, Serialize)]
struct IndexerStatusResponse {
    available: bool,
    local_configured: bool,
    public_configured: bool,
}

/// `GET /api/node/status` — current algod node status (with public fallback).
async fn node_status(
    State(state): State<AppState>,
) -> Result<Json<NodeStatusResponse>, (StatusCode, Json<ApiError>)> {
    let (_, status, source) = state.authoritative_ledger().await.map_err(|e| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new(format!("node unreachable: {e}"))),
        )
    })?;

    if source == DataSource::Local {
        state
            .runtime
            .sync_tracker
            .record(status.last_round.0, status.is_caught_up())
            .await;
    }

    Ok(Json(NodeStatusResponse {
        last_round: status.last_round.0,
        last_version: status.last_version,
        time_since_last_round: status.time_since_last_round,
        catchup_time: status.catchup_time,
        source,
    }))
}

/// `GET /api/node/sync-progress` — estimated sync progress and ETA.
async fn sync_progress(
    State(state): State<AppState>,
) -> Result<Json<crate::sync::SyncProgress>, (StatusCode, Json<ApiError>)> {
    let status = state.ledger.algod.status().await.map_err(|e| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new(format!("node unreachable: {e}"))),
        )
    })?;
    let progress = state
        .runtime
        .sync_tracker
        .progress(status.last_round.0, status.is_caught_up())
        .await;
    Ok(Json(progress))
}

/// `GET /api/node/block-info` — recent block headers.
async fn block_info(State(state): State<AppState>) -> Json<Vec<crate::participation::BlockInfo>> {
    Json(state.runtime.participation_tracker.block_info().await)
}

/// `GET /api/node/participation-stats` — node block-proposal participation stats.
async fn participation_stats(
    State(state): State<AppState>,
) -> Json<crate::participation::ParticipationStats> {
    Json(
        state
            .runtime
            .participation_tracker
            .participation_stats()
            .await,
    )
}

/// `GET /api/indexer/status` — whether the indexer is available and synced.
async fn indexer_status(State(state): State<AppState>) -> Json<IndexerStatusResponse> {
    let available = state.has_indexer();
    let local_configured = state.ledger.indexer.is_some();
    let public_configured = state.ledger.public_indexer.is_some();
    Json(IndexerStatusResponse {
        available,
        local_configured,
        public_configured,
    })
}

/// `GET /api/indexer/sync-progress` — indexer synchronization progress.
async fn indexer_sync_progress(
    State(state): State<AppState>,
) -> Json<crate::sync::IndexerSyncProgress> {
    Json(state.runtime.indexer_sync_tracker.progress().await)
}
