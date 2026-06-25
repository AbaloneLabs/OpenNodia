use std::collections::HashSet;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use opennodia_core::Address;
use opennodia_node::{DataSource, IndexerTransaction};
use serde::{Deserialize, Serialize};

use crate::api_error::ApiError;
use crate::state::AppState;

use super::account_routes::{asset_authority_enabled, fetch_asset_params};
use super::history_routes::{transaction_entry_from_indexer, TransactionEntry};

pub(super) fn analytics_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/analytics/assets/creator/{creator}",
            get(analytics_assets_by_creator),
        )
        .route(
            "/api/analytics/assets/{asset_id}/holders",
            get(analytics_asset_holders),
        )
        .route(
            "/api/analytics/assets/{asset_id}/applications",
            get(analytics_asset_applications),
        )
        .route(
            "/api/analytics/assets/{asset_id}/transactions",
            get(analytics_asset_transactions),
        )
}

#[derive(Debug, Deserialize)]
struct AnalyticsAssetQuery {
    limit: Option<u32>,
    min_round: Option<u64>,
    max_round: Option<u64>,
    tx_type: Option<String>,
    policy: Option<String>,
    min_volume: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct AnalyticsHoldersQuery {
    limit: Option<u32>,
    min_balance: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct AnalyticsLimitQuery {
    limit: Option<u32>,
}

#[derive(Debug, Serialize)]
struct AnalyticsContextResponse {
    source: DataSource,
    sources: Vec<DataSource>,
    indexed_round: Option<u64>,
    network_round: Option<u64>,
    rounds_behind: Option<u64>,
    synced: bool,
}

#[derive(Debug, Serialize)]
struct AnalyticsAssetEntry {
    asset_id: u64,
    name: String,
    unit: String,
    decimals: u32,
    total: u64,
    creator: String,
    policy: String,
    volume: Option<u64>,
}

#[derive(Debug, Serialize)]
struct AnalyticsAssetsResponse {
    creator: String,
    assets: Vec<AnalyticsAssetEntry>,
    context: AnalyticsContextResponse,
}

#[derive(Debug, Serialize)]
struct AnalyticsHolderEntry {
    address: String,
    amount: u64,
    round: u64,
}

#[derive(Debug, Serialize)]
struct AnalyticsHoldersResponse {
    asset_id: u64,
    holders: Vec<AnalyticsHolderEntry>,
    total_returned: u64,
    context: AnalyticsContextResponse,
}

#[derive(Debug, Serialize)]
struct AnalyticsApplicationEntry {
    app_id: u64,
    creator: String,
    created_at_round: u64,
    global_state_keys: usize,
    lp_pool_candidate: bool,
}

#[derive(Debug, Serialize)]
struct AnalyticsApplicationsResponse {
    asset_id: u64,
    applications: Vec<AnalyticsApplicationEntry>,
    context: AnalyticsContextResponse,
}

#[derive(Debug, Serialize)]
struct AnalyticsAssetTransactionsResponse {
    asset_id: u64,
    policy: String,
    volume: u64,
    transactions: Vec<TransactionEntry>,
    context: AnalyticsContextResponse,
}

/// `GET /api/analytics/assets/creator/:creator` — assets created by account.
async fn analytics_assets_by_creator(
    State(state): State<AppState>,
    Path(creator): Path<String>,
    Query(query): Query<AnalyticsAssetQuery>,
) -> Result<Json<AnalyticsAssetsResponse>, (StatusCode, Json<ApiError>)> {
    let creator = creator.parse::<Address>().map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(format!("invalid creator address: {e}"))),
        )
    })?;
    let creator = creator.to_string();
    validate_analytics_query(&query)?;
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let (indexer, source, context) = analytics_indexer(&state).await?;
    let response = indexer
        .assets_by_creator(&creator, limit)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(ApiError::new(format!("creator asset search failed: {e}"))),
            )
        })?;

    let mut assets = Vec::new();
    for asset in response
        .assets
        .into_iter()
        .filter(|asset| asset.destroyed_at_round == 0)
    {
        let policy = policy_from_params(&asset.params).map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(ApiError::new(format!("asset policy decode failed: {e}"))),
            )
        })?;
        if let Some(expected) = normalized_policy_filter(query.policy.as_deref())? {
            if policy != expected {
                continue;
            }
        }
        let volume = if query.min_volume.is_some() {
            let (transactions, _) =
                combined_asset_transactions(&state, asset.index, &query).await?;
            let volume = asset_transfer_volume(asset.index, &transactions);
            if query.min_volume.is_some_and(|minimum| volume < minimum) {
                continue;
            }
            Some(volume)
        } else {
            None
        };
        assets.push(AnalyticsAssetEntry {
            asset_id: asset.index,
            name: if asset.params.name.is_empty() {
                format!("Asset #{}", asset.index)
            } else {
                asset.params.name
            },
            unit: if asset.params.unit_name.is_empty() {
                format!("#{}", asset.index)
            } else {
                asset.params.unit_name
            },
            decimals: asset.params.decimals,
            total: asset.params.total,
            creator: asset.params.creator,
            policy: policy.to_string(),
            volume,
        });
    }

    Ok(Json(AnalyticsAssetsResponse {
        creator,
        assets,
        context: context.with_source(source),
    }))
}

/// `GET /api/analytics/assets/:asset_id/holders` — asset holder search.
async fn analytics_asset_holders(
    State(state): State<AppState>,
    Path(asset_id): Path<u64>,
    Query(query): Query<AnalyticsHoldersQuery>,
) -> Result<Json<AnalyticsHoldersResponse>, (StatusCode, Json<ApiError>)> {
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let (indexer, source, context) = analytics_indexer(&state).await?;
    let response = indexer
        .accounts_by_asset(asset_id, limit)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(ApiError::new(format!("asset holder search failed: {e}"))),
            )
        })?;
    let mut holders = response
        .accounts
        .into_iter()
        .filter_map(|account| {
            let amount = account
                .assets
                .iter()
                .find(|holding| holding.asset_id == asset_id)
                .map(|holding| holding.amount)
                .unwrap_or(0);
            if query.min_balance.is_some_and(|minimum| amount < minimum) {
                return None;
            }
            Some(AnalyticsHolderEntry {
                address: account.address,
                amount,
                round: account.round,
            })
        })
        .collect::<Vec<_>>();
    holders.sort_by_key(|holder| std::cmp::Reverse(holder.amount));
    Ok(Json(AnalyticsHoldersResponse {
        asset_id,
        total_returned: holders.len() as u64,
        holders,
        context: context.with_source(source),
    }))
}

/// `GET /api/analytics/assets/:asset_id/applications` — apps involving asset.
async fn analytics_asset_applications(
    State(state): State<AppState>,
    Path(asset_id): Path<u64>,
    Query(query): Query<AnalyticsLimitQuery>,
) -> Result<Json<AnalyticsApplicationsResponse>, (StatusCode, Json<ApiError>)> {
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let (indexer, source, context) = analytics_indexer(&state).await?;
    let applications = indexer
        .applications_by_asset_limited(asset_id, limit)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(ApiError::new(format!(
                    "asset application search failed: {e}"
                ))),
            )
        })?
        .into_iter()
        .map(|app| AnalyticsApplicationEntry {
            app_id: app.id,
            creator: app.params.creator,
            created_at_round: app.created_at_round,
            global_state_keys: app.params.global_state.len(),
            lp_pool_candidate: app.params.global_state.len() >= 2,
        })
        .collect();
    Ok(Json(AnalyticsApplicationsResponse {
        asset_id,
        applications,
        context: context.with_source(source),
    }))
}

/// `GET /api/analytics/assets/:asset_id/transactions` — volume and filtered transactions.
async fn analytics_asset_transactions(
    State(state): State<AppState>,
    Path(asset_id): Path<u64>,
    Query(query): Query<AnalyticsAssetQuery>,
) -> Result<Json<AnalyticsAssetTransactionsResponse>, (StatusCode, Json<ApiError>)> {
    validate_analytics_query(&query)?;
    let params = fetch_asset_params(&state, asset_id)
        .await
        .map_err(|error| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiError::new(format!(
                    "asset {asset_id} lookup failed: {error}"
                ))),
            )
        })?;
    let policy = policy_from_params(&params).map_err(|error| {
        (
            StatusCode::BAD_GATEWAY,
            Json(ApiError::new(format!(
                "asset policy decode failed: {error}"
            ))),
        )
    })?;
    if let Some(expected) = normalized_policy_filter(query.policy.as_deref())? {
        if policy != expected {
            return Ok(Json(AnalyticsAssetTransactionsResponse {
                asset_id,
                policy: policy.to_string(),
                volume: 0,
                transactions: Vec::new(),
                context: analytics_context(&state, Vec::new()).await,
            }));
        }
    }

    let (transactions, sources) = combined_asset_transactions(&state, asset_id, &query).await?;
    let volume = asset_transfer_volume(asset_id, &transactions);
    let entries = transactions
        .into_iter()
        .map(transaction_entry_from_indexer)
        .collect();
    Ok(Json(AnalyticsAssetTransactionsResponse {
        asset_id,
        policy: policy.to_string(),
        volume,
        transactions: entries,
        context: analytics_context(&state, sources).await,
    }))
}

async fn analytics_indexer(
    state: &AppState,
) -> Result<
    (
        &opennodia_node::IndexerClient,
        DataSource,
        AnalyticsContextResponse,
    ),
    (StatusCode, Json<ApiError>),
> {
    let Some((indexer, source)) = state.effective_search_client().await else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new("Indexer is not configured")),
        ));
    };
    Ok((
        indexer,
        source,
        analytics_context(state, vec![source]).await,
    ))
}

async fn analytics_context(state: &AppState, sources: Vec<DataSource>) -> AnalyticsContextResponse {
    let progress = state.runtime.indexer_sync_tracker.progress().await;
    let source = sources.first().copied().unwrap_or(DataSource::Public);
    AnalyticsContextResponse {
        source,
        sources,
        indexed_round: progress.indexed_round,
        network_round: progress.network_round,
        rounds_behind: progress.rounds_behind,
        synced: progress.synced,
    }
}

impl AnalyticsContextResponse {
    fn with_source(mut self, source: DataSource) -> Self {
        self.source = source;
        if self.sources.is_empty() {
            self.sources.push(source);
        }
        self
    }
}

async fn combined_asset_transactions(
    state: &AppState,
    asset_id: u64,
    query: &AnalyticsAssetQuery,
) -> Result<(Vec<IndexerTransaction>, Vec<DataSource>), (StatusCode, Json<ApiError>)> {
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let tx_type = normalized_tx_type(query.tx_type.as_deref())?;
    let mut transactions = Vec::new();
    let mut sources = Vec::new();
    let mut local_covers_genesis = false;
    let mut public_max_round = query.max_round;

    if state.local_history_ready().await {
        if let Some(local) = state.ledger.indexer.as_ref() {
            if let Ok(health) = local.health().await {
                let retention_floor = health.round.saturating_sub(
                    state
                        .config
                        .indexer
                        .history_retention_rounds
                        .max(1)
                        .saturating_sub(1),
                );
                let local_min = query
                    .min_round
                    .unwrap_or(retention_floor)
                    .max(retention_floor);
                let local_max = query.max_round;
                if local_max.is_none_or(|max_round| local_min <= max_round) {
                    match local
                        .asset_transactions_page(
                            asset_id,
                            limit,
                            Some(local_min),
                            local_max,
                            tx_type,
                            None,
                        )
                        .await
                    {
                        Ok(page) => {
                            transactions.extend(page.transactions);
                            sources.push(DataSource::Local);
                            public_max_round = retention_floor.checked_sub(1).and_then(|floor| {
                                query
                                    .max_round
                                    .map_or(Some(floor), |max_round| Some(max_round.min(floor)))
                            });
                            local_covers_genesis = retention_floor == 0;
                        }
                        Err(error) => {
                            tracing::debug!(
                                asset_id,
                                %error,
                                "local asset transaction analysis failed"
                            );
                        }
                    }
                }
            }
        }
    }

    let need_public = transactions.len() < limit as usize && !local_covers_genesis;
    if need_public {
        if let Some(public) = state.ledger.public_indexer.as_ref() {
            let remaining = limit.saturating_sub(transactions.len() as u32);
            if remaining > 0 {
                match public
                    .asset_transactions_page(
                        asset_id,
                        remaining,
                        query.min_round,
                        public_max_round,
                        tx_type,
                        None,
                    )
                    .await
                {
                    Ok(page) => {
                        transactions.extend(page.transactions);
                        sources.push(DataSource::Public);
                    }
                    Err(error) if !transactions.is_empty() => {
                        tracing::warn!(
                            asset_id,
                            %error,
                            "public asset transaction fallback failed"
                        );
                    }
                    Err(error) => {
                        return Err((
                            StatusCode::BAD_GATEWAY,
                            Json(ApiError::new(format!(
                                "asset transaction analysis failed: {error}"
                            ))),
                        ));
                    }
                }
            }
        } else if transactions.is_empty() {
            let Some((indexer, source)) = state.effective_search_client().await else {
                return Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(ApiError::new("Indexer is not configured")),
                ));
            };
            let page = indexer
                .asset_transactions_page(
                    asset_id,
                    limit,
                    query.min_round,
                    query.max_round,
                    tx_type,
                    None,
                )
                .await
                .map_err(|error| {
                    (
                        StatusCode::BAD_GATEWAY,
                        Json(ApiError::new(format!(
                            "asset transaction analysis failed: {error}"
                        ))),
                    )
                })?;
            transactions.extend(page.transactions);
            sources.push(source);
        }
    }

    dedupe_sort_transactions(&mut transactions);
    transactions.truncate(limit as usize);
    Ok((transactions, sources))
}

fn validate_analytics_query(
    query: &AnalyticsAssetQuery,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    if matches!((query.min_round, query.max_round), (Some(min), Some(max)) if min > max) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                "min_round must be less than or equal to max_round",
            )),
        ));
    }
    normalized_tx_type(query.tx_type.as_deref())?;
    normalized_policy_filter(query.policy.as_deref())?;
    Ok(())
}

fn normalized_tx_type(value: Option<&str>) -> Result<Option<&str>, (StatusCode, Json<ApiError>)> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    const ALLOWED: &[&str] = &["pay", "axfer", "afrz", "keyreg", "acfg", "appl"];
    if ALLOWED.contains(&value) {
        Ok(Some(value))
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(format!("unsupported tx_type: {value}"))),
        ))
    }
}

fn normalized_policy_filter(
    value: Option<&str>,
) -> Result<Option<&'static str>, (StatusCode, Json<ApiError>)> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    match value {
        "all" => Ok(None),
        "open" => Ok(Some("open")),
        "bridged" => Ok(Some("bridged")),
        "regulated" => Ok(Some("regulated")),
        other => Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(format!("unsupported policy: {other}"))),
        )),
    }
}

fn policy_from_params(params: &opennodia_node::AssetParams) -> anyhow::Result<&'static str> {
    let grade = opennodia_assets::AssetPolicyGrade::classify(
        asset_authority_enabled(&params.freeze)?,
        asset_authority_enabled(&params.clawback)?,
        params.default_frozen,
    );
    Ok(match grade {
        opennodia_assets::AssetPolicyGrade::Open => "open",
        opennodia_assets::AssetPolicyGrade::Bridged => "bridged",
        opennodia_assets::AssetPolicyGrade::Regulated => "regulated",
    })
}

fn asset_transfer_volume(asset_id: u64, transactions: &[IndexerTransaction]) -> u64 {
    transactions.iter().fold(0u64, |total, transaction| {
        let amount = transaction
            .asset_transfer
            .as_ref()
            .filter(|transfer| transfer.asset_id == asset_id)
            .map(|transfer| transfer.amount)
            .unwrap_or(0);
        total.saturating_add(amount)
    })
}

fn dedupe_sort_transactions(transactions: &mut Vec<IndexerTransaction>) {
    let mut seen = HashSet::new();
    transactions.retain(|transaction| {
        let key = if transaction.id.is_empty() {
            format!(
                "{}:{}:{}",
                transaction.round, transaction.intra_round_offset, transaction.tx_type
            )
        } else {
            transaction.id.clone()
        };
        seen.insert(key)
    });
    transactions.sort_by(|left, right| {
        right
            .round
            .cmp(&left.round)
            .then_with(|| right.intra_round_offset.cmp(&left.intra_round_offset))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedupe_sort_transactions_keeps_newest_unique_rows() {
        let mut transactions = vec![
            IndexerTransaction {
                id: "old".into(),
                round: 10,
                intra_round_offset: 1,
                tx_type: "axfer".into(),
                ..serde_json::from_value(serde_json::json!({})).unwrap()
            },
            IndexerTransaction {
                id: "new".into(),
                round: 12,
                intra_round_offset: 0,
                tx_type: "axfer".into(),
                ..serde_json::from_value(serde_json::json!({})).unwrap()
            },
            IndexerTransaction {
                id: "old".into(),
                round: 10,
                intra_round_offset: 1,
                tx_type: "axfer".into(),
                ..serde_json::from_value(serde_json::json!({})).unwrap()
            },
        ];
        dedupe_sort_transactions(&mut transactions);
        assert_eq!(transactions.len(), 2);
        assert_eq!(transactions[0].id, "new");
        assert_eq!(transactions[1].id, "old");
    }
}
